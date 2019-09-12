use actix_web::{
    HttpRequest,
    HttpResponse,
    http::StatusCode,
    guard,
    web::{self, Data, Form, Query, ServiceConfig},
};
use adaptarr_error::ApiError;
use adaptarr_i18n::{I18n, LanguageTag};
use adaptarr_models::{
    User,
    Model,
    PasswordResetToken,
    UserFields,
    Invite,
    audit,
};
use adaptarr_web::{Secret, Locale, Database, session::{Session, Normal}};
use log::warn;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashMap};

adaptarr_i18n::localized_templates!(PAGES = "templates/pages/**/*");

type Result<T, E=adaptarr_error::Error> = std::result::Result<T, E>;

/// Configure routes
pub fn configure(app: &mut ServiceConfig) {
    app
        .service(web::resource("/login")
            .route(web::get().to(login))
            .route(web::post().to(do_login))
        )
        .service(web::resource("/elevate")
            .route(web::get().to(elevate))
            .route(web::post()
                .guard(guard::Header("Accept", "application/json"))
                .to(do_elevate_json))
            .route(web::post().to(do_elevate))
        )
        .route("/logout", web::get()./**/to(logout))
        .service(web::resource("/reset")
            .name("reset")
            .route(web::get().to(reset))
            .route(web::post().to(do_reset))
        )
        .service(web::resource("/register")
            .name("register")
            .route(web::get().to(register))
            .route(web::post().to(do_register))
        )
    ;
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LoginAction {
    /// Redirect to the URl specified in `next`.
    Next,
    /// Use `window.postMessage()` to notify opener.
    Message,
}

impl Default for LoginAction {
    fn default() -> LoginAction {
        LoginAction::Next
    }
}

#[derive(Deserialize, Serialize)]
struct LoginQuery {
    next: Option<String>,
    #[serde(default)]
    action: LoginAction,
}

#[derive(Serialize)]
struct LoginTemplate<'error> {
    error: Option<Cow<'error, str>>,
    next: Option<String>,
    action: LoginAction,
}

/// Render a login screen.
///
/// ## Method
///
/// ```text
/// GET /login
/// ```
fn login(
    session: Option<Session>,
    locale: Locale<'static>,
    query: Query<LoginQuery>,
) -> Result<HttpResponse> {
    if session.is_some() {
        return Ok(HttpResponse::SeeOther()
            .header("Location", query.next.as_ref().map_or("/", String::as_str))
            .finish());
    }

    let LoginQuery { next, action } = query.into_inner();

    render(locale.as_ref(), "login.html", &LoginTemplate {
        error: None,
        next,
        action,
    })
}

#[derive(Deserialize)]
struct LoginParams {
    email: String,
    password: String,
    next: Option<String>,
}

/// Perform login.
///
/// ## Method
///
/// ```text
/// POST /login
/// ```
fn do_login(
    req: HttpRequest,
    db: Database,
    locale: Locale<'static>,
    params: Form<LoginParams>,
) -> Result<HttpResponse> {
    let LoginParams { email, password, next } = params.into_inner();

    let user = match User::authenticate(&db, &email, &password) {
        Ok(user) => user,
        Err(err) => match err.code() {
            None => return Err(err.into()),
            Some(code) => return render_code(
                locale.as_ref(),
                StatusCode::BAD_REQUEST,
                "login.html",
                &LoginTemplate {
                    error: Some(code),
                    action: LoginAction::default(),
                    next,
                },
            ),
        }
    };

    audit::log_db_actor(&*db, user.id, "users", user.id, "authenticate", ());

    // NOTE: This will automatically remove any session that may still exist,
    // we don't have to do it manually here.
    Session::<Normal>::create(&req, &user, false);

    Ok(HttpResponse::SeeOther()
        .header("Location", next.as_ref().map_or("/", String::as_str))
        .finish())
}

/// Render a session elevation screen.
///
/// ## Method
///
/// ```text
/// GET /elevate
/// ```
fn elevate(locale: Locale<'static>, query: Query<LoginQuery>) -> Result<HttpResponse> {
    let LoginQuery { next, action } = query.into_inner();

    render(locale.as_ref(), "elevate.html", &LoginTemplate {
        error: None,
        next,
        action,
    })
}

#[derive(Deserialize)]
struct ElevateCredentials {
    password: String,
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    action: LoginAction,
}

/// Perform session elevation.
///
/// ## Method
///
/// ```text
/// POST /elevate
/// ```
fn do_elevate(
    req: HttpRequest,
    db: Database,
    session: Session,
    locale: Locale<'static>,
    form: Form<ElevateCredentials>,
) -> Result<HttpResponse> {
    let user = session.user(&db)?;
    let ElevateCredentials { next, action, password } = form.into_inner();

    if !user.check_password(&password) {
        return render_code(
            locale.as_ref(),
            StatusCode::BAD_REQUEST,
            "elevate.html",
            &LoginTemplate {
                error: Some(Cow::from("user:authenticate:bad-password")),
                next,
                action,
            },
        );
    }

    audit::log_db(&*db, "users", user.id, "elevate", ());

    Session::<Normal>::create(&req, &user, true);

    Ok(HttpResponse::SeeOther()
        .header("Location", next.as_ref().map_or("/", String::as_str))
        .finish())
}

#[derive(Serialize)]
#[serde(untagged)]
enum ElevationResult<'a> {
    Error {
        message: Cow<'a, str>,
    },
    Success,
}

/// Perform session elevation, returning response as JSON.
///
/// ## Method
///
/// ```text
/// POST /elevate
/// Accept: application/json
/// ```
fn do_elevate_json(
    req: HttpRequest,
    db: Database,
    session: Session,
    locale: Locale<'static>,
    form: Form<ElevateCredentials>,
) -> Result<HttpResponse> {
    let user = session.user(&db)?;
    let ElevateCredentials { password, .. } = form.into_inner();

    if !user.check_password(&password) {
        let mut args = HashMap::new();
        args.insert("code", "user:authenticate:bad-password".into());

        let message = match locale.format("elevate-error", &args) {
            Some(message) => message,
            None => {
                warn!("Message elevate-error missing from locale {}",
                    locale.code);
                Cow::from("")
            }
        };

        return Ok(HttpResponse::BadRequest()
            .json(ElevationResult::Error { message }));
    }

    audit::log_db(&*db, "users", user.id, "elevate", ());

    Session::<Normal>::create(&req, &user, true);

    Ok(HttpResponse::Ok().json(ElevationResult::Success))
}

// TODO: replace with tera::Context::new()
/// Empty serializable structure to serve as empty context.
#[derive(Serialize)]
struct Empty {
}


/// Log an user out and destroy their session.
///
/// ## Method
///
/// ```text
/// GET /logout
/// ```
fn logout(req: HttpRequest, session: Session, locale: Locale<'static>) -> Result<HttpResponse> {
    Session::destroy(&req, session);
    render(locale.as_ref(), "logout.html", &Empty {})
}

#[derive(Deserialize, Serialize)]
struct ResetQuery {
    token: Option<String>,
}

#[derive(Serialize)]
struct ResetTemplate<'s> {
    error: Option<Cow<'s, str>>,
    token: Option<&'s str>,
}

/// Request a password reset or render a reset form (with a token).
///
/// ## Method
///
/// ```text
/// GET /reset
/// ```
fn reset(query: Query<ResetQuery>, locale: Locale<'static>) -> Result<HttpResponse> {
    render(locale.as_ref(), "reset.html", &*query)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ResetForm {
    CreateToken {
        email: String,
    },
    FulfilToken {
        password: String,
        password1: String,
        token: String,
    },
}

/// Arguments for `mail/reset`.
#[derive(Serialize)]
struct ResetMailArgs<'a> {
    /// User to whom the email is sent.
    user: <User as Model>::Public,
    /// Password reset URL.
    url: &'a str,
}

/// Send reset token in an e-mail or perform password reset (with a token).
///
/// ## Method
///
/// ```text
/// POST /reset
/// ```
fn do_reset(
    req: HttpRequest,
    db: Database,
    i18n: Data<I18n<'static>>,
    secret: Data<Secret>,
    locale: Locale<'static>,
    form: Form<ResetForm>,
) -> Result<HttpResponse> {
    match form.into_inner() {
        ResetForm::CreateToken { email } => {
            let user = match User::by_email(&db, &email) {
                Ok(user) => user,
                Err(error) => {
                    if let Some(code) = error.code() {
                        return render_code(
                            locale.as_ref(),
                            StatusCode::BAD_REQUEST,
                            "reset.html",
                            &ResetTemplate {
                                error: Some(code),
                                token: None,
                            },
                        );
                    }

                    return Err(error.into());
                }
            };
            let token = PasswordResetToken::create(&db, &user)?;

            let code = token.get_code(&secret);
            let mut url = req.url_for_static("reset")?;
            url.query_pairs_mut().append_pair("token", &code);

            user.do_send_mail("reset", "mail-reset-subject", ResetMailArgs {
                user: user.get_public_full(&db, &UserFields::empty())?,
                url: url.as_str(),
            });

            render(locale.as_ref(), "reset_token_sent.html", &Empty {})
        }
        ResetForm::FulfilToken { password, password1, token: token_str } => {
            let token = match PasswordResetToken::from_code(
                &db, &secret, &token_str)
            {
                Ok(token) => token,
                Err(error) => {
                    if let Some(code) = error.code() {
                        return render_code(
                            locale.as_ref(),
                            StatusCode::BAD_REQUEST,
                            "reset.html",
                            &ResetTemplate {
                                error: Some(code),
                                token: None,
                            },
                        );
                    }

                    return Err(error.into());
                }
            };

            if password != password1 {
                return render_code(
                    locale.as_ref(),
                    StatusCode::BAD_REQUEST,
                    "reset.html",
                    &ResetTemplate {
                        error: Some(Cow::from("password:reset:passwords-dont-match")),
                        token: Some(&token_str),
                    },
                );
            }

            let user = match token.fulfil(&*db, &password) {
                Ok(user) => user,
                Err(err) => {
                    if err.code().is_none() {
                        return Err(err.into());
                    }

                    return render_code(
                        locale.as_ref(),
                        err.status(),
                        "reset.html",
                        &ResetTemplate {
                            error: err.code(),
                            token: Some(&token_str),
                        },
                    );
                }
            };

            Session::<Normal>::create(&req, &user, false);

            Ok(HttpResponse::SeeOther().header("Location", "/").finish())
        }
    }
}

#[derive(Deserialize, Serialize)]
struct RegisterQuery {
    invite: String,
}

#[derive(Serialize)]
struct RegisterTemplate<'s> {
    error: Option<Cow<'s, str>>,
    email: &'s str,
    invite: &'s str,
    /// List of all available locales.
    locales: &'s [adaptarr_i18n::Locale],
    /// Locale in which the request was sent. Used as default value for locale
    /// selector.
    locale: &'s adaptarr_i18n::Locale,
}

/// Render registration form.
///
/// ## Method
///
/// ```text
/// GET /register
/// ```
fn register(
    db: Database,
    i18n: Data<I18n>,
    secret: Data<Secret>,
    locale: Locale<'static>,
    query: Query<RegisterQuery>,
) -> Result<HttpResponse> {
    let invite = Invite::from_code(&db, &secret, &query.invite)?;

    render(locale.as_ref(), "register.html", &RegisterTemplate {
        error: None,
        email: &invite.email,
        invite: &query.invite,
        locales: &i18n.locales,
        locale: locale.as_ref(),
    })
}

#[derive(Deserialize)]
struct RegisterForm {
    email: String,
    name: String,
    password: String,
    password1: String,
    invite: String,
    language: LanguageTag,
}

/// Perform registration.
///
/// ## Method
///
/// ```text
/// POST /register
/// ```
fn do_register(
    req: HttpRequest,
    db: Database,
    i18n: Data<I18n>,
    secret: Data<Secret>,
    locale: Locale<'static>,
    form: Form<RegisterForm>,
) -> Result<HttpResponse> {
    let RegisterForm {
        email, name, password, password1, invite: invite_code, language,
    } = form.into_inner();

    #[allow(clippy::or_fun_call)]
    let requested_locale = i18n.find_locale(&language).unwrap_or(locale.as_ref());

    let invite = match Invite::from_code(&db, &secret, &invite_code) {
        Ok(invite) => invite,
        Err(error) => {
            if let Some(code) = error.code() {
                return render_code(
                    locale.as_ref(),
                    StatusCode::BAD_REQUEST,
                    "register.html",
                    &RegisterTemplate {
                        error: Some(code),
                        email: &email,
                        invite: &invite_code,
                        locales: &i18n.locales,
                        locale: requested_locale,
                    }
                );
            }

            return Err(error.into());
        }
    };

    if password != password1 {
        return render_code(
            locale.as_ref(),
            StatusCode::BAD_REQUEST,
            "register.html",
            &RegisterTemplate {
                error: Some(Cow::from("user:password:bad-confirmation")),
                email: &invite.email,
                invite: &invite_code,
                locales: &i18n.locales,
                locale: requested_locale,
            },
        );
    }

    if email != invite.email {
        return render_code(
            locale.as_ref(),
            StatusCode::BAD_REQUEST,
            "register.html",
            &RegisterTemplate {
                error: Some(Cow::from("user:register:email-changed")),
                email: &invite.email,
                invite: &invite_code,
                locales: &i18n.locales,
                locale: requested_locale,
            },
        );
    }

    let user = match invite.fulfil(
        &db,
        &name,
        &password,
        &requested_locale.code.to_string(),
    ) {
        Ok(user) => user,
        Err(err) => {
            if err.code().is_none(){
                return Err(err.into());
            }

            return render_code(
                locale.as_ref(),
                err.status(),
                "register.html",
                &RegisterTemplate {
                    error: Some(err.code().unwrap()),
                    email: &email,
                    invite: &invite_code,
                    locales: &i18n.locales,
                    locale: requested_locale,
                },
            );
        }
    };

    Session::<Normal>::create(&req, &user, false);

    Ok(HttpResponse::SeeOther().header("Location", "/").finish())
}

/// Render a named template with a given context.
///
/// This is a small wrapper around [`Tera::render`] which also handles errors
/// and transforms them into a usable response.
fn render<T>(locale: &'static adaptarr_i18n::Locale, name: &str, context: &T)
-> Result<HttpResponse>
where
    T: serde::Serialize,
{
    render_code(locale, StatusCode::OK, name, context)
}

/// Render a named template with a given context and given status code.
///
/// This is a small wrapper around [`Tera::render`] which also handles errors
/// and transforms them into a usable response.
fn render_code<T>(
    locale: &'static adaptarr_i18n::Locale,
    code: StatusCode,
    name: &str,
    context: &T,
) -> Result<HttpResponse>
where
    T: serde::Serialize,
{
    PAGES.render_i18n(name, context, locale)
        .map(|r| HttpResponse::build(code).body(r))
        // .map_err(Into::into)
        .map(Ok).expect("TODO: errors")
}
