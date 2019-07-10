use actix_web::{
    App,
    Form,
    HttpRequest,
    HttpResponse,
    Query,
    http::{Method, StatusCode},
    middleware::Logger,
    pred,
};
use sentry_actix::SentryMiddleware;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    i18n::{LanguageTag, Locale},
    mail::Mailer,
    models::{
        Invite,
        PasswordResetToken,
        user::{User, Fields as UserFields},
    },
    templates,
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    error::ApiError,
    session::{SessionManager, Session, Normal},
};

pub fn app(state: State) -> App<State> {
    App::with_state(state)
        .middleware(SentryMiddleware::new())
        .middleware(Logger::default())
        .middleware(SessionManager::new())
        .resource("/login", |r| {
            r.get().api_with(login);
            r.post().api_with(do_login);
        })
        .resource("/elevate", |r| {
            r.get().api_with(elevate);
            r.post()
                .filter(pred::Header("Accept", "application/json"))
                .api_with(do_elevate_json);
            r.post().api_with(do_elevate);
        })
        .api_route("/logout", Method::GET, logout)
        .resource("/reset", |r| {
            r.get().api_with(reset);
            r.post().api_with(do_reset);
        })
        .resource("/register", |r| {
            r.name("register");
            r.get().api_with(register);
            r.post().api_with(do_register);
        })
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LoginAction {
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

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginQuery {
    next: Option<String>,
    #[serde(default)]
    action: LoginAction,
}

#[derive(Debug, Serialize)]
struct LoginTemplate<'error> {
    error: Option<&'error str>,
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
pub fn login(
    session: Option<Session>,
    locale: &'static Locale<'static>,
    query: Query<LoginQuery>,
) -> RenderedTemplate {
    if session.is_some() {
        return Ok(HttpResponse::SeeOther()
            .header("Location", query.next.as_ref().map_or("/", String::as_str))
            .finish());
    }

    let LoginQuery { next, action } = query.into_inner();

    render(locale, "login.html", &LoginTemplate {
        error: None,
        next,
        action,
    })
}

#[derive(Debug, Deserialize)]
pub struct LoginCredentials {
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
pub fn do_login(
    req: HttpRequest<State>,
    locale: &'static Locale<'static>,
    params: Form<LoginCredentials>,
) -> RenderedTemplate {
    let db = &*req.state().db.get()?;

    let user = match User::authenticate(db, &params.email, &params.password) {
        Ok(user) => user,
        Err(err) => {
            if let Some(code) = err.code() {
                return render_code(
                    locale,
                    StatusCode::BAD_REQUEST,
                    "login.html",
                    &LoginTemplate {
                        error: Some(code),
                        next: params.into_inner().next,
                        action: LoginAction::default(),
                    },
                );
            }

            return Err(err.into());
        },
    };

    // NOTE: This will automatically remove any session that may still exist,
    // we don't have to do it manually here.
    Session::<Normal>::create(&req, &user, false);

    Ok(HttpResponse::SeeOther()
        .header("Location", params.next.as_ref().map_or("/", String::as_str))
        .finish())
}

/// Render a session elevation screen.
///
/// ## Method
///
/// ```text
/// GET /elevate
/// ```
pub fn elevate(
    locale: &'static Locale<'static>,
    query: Query<LoginQuery>,
) -> RenderedTemplate {
    let LoginQuery { next, action } = query.into_inner();

    render(locale, "elevate.html", &LoginTemplate {
        error: None,
        next,
        action,
    })
}

#[derive(Debug, Deserialize)]
pub struct ElevateCredentials {
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
pub fn do_elevate(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    session: Session,
    locale: &'static Locale<'static>,
    form: Form<ElevateCredentials>,
) -> RenderedTemplate {
    let db = state.db.get()?;
    let user = User::by_id(&*db, session.user)?;
    let ElevateCredentials { next, action, password } = form.into_inner();

    if !user.check_password(&password) {
        return render_code(
            locale,
            StatusCode::BAD_REQUEST,
            "elevate.html",
            &LoginTemplate {
                error: Some("user:authenticate:bad-password"),
                next,
                action,
            },
        );
    }

    Session::<Normal>::create(&req, &user, true);

    Ok(HttpResponse::SeeOther()
        .header("Location", next.as_ref().map_or("/", String::as_str))
        .finish())
}

#[derive(Serialize)]
#[serde(untagged)]
enum ElevationResult {
    Error {
        message: String,
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
pub fn do_elevate_json(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    session: Session,
    locale: &'static Locale<'static>,
    form: Form<ElevateCredentials>,
) -> RenderedTemplate {
    let db = state.db.get()?;
    let user = User::by_id(&*db, session.user)?;
    let ElevateCredentials { password, .. } = form.into_inner();

    if !user.check_password(&password) {
        let mut args = HashMap::new();
        args.insert("code", "user:authenticate:bad-password".into());

        let message = match locale.format("elevate-error", &args) {
            Some(message) => message,
            None => {
                warn!("Message elevate-error missing from locale {}",
                    locale.code);
                "".to_string()
            }
        };

        return Ok(HttpResponse::BadRequest()
            .json(ElevationResult::Error { message }));
    }

    Session::<Normal>::create(&req, &user, true);

    Ok(HttpResponse::Ok().json(ElevationResult::Success))
}

/// Log an user out and destroy their session.
///
/// ## Method
///
/// ```text
/// GET /logout
/// ```
pub fn logout(
    req: HttpRequest<State>,
    sess: Session,
    locale: &'static Locale<'static>
) -> RenderedTemplate {
    Session::destroy(&req, sess);
    render(locale, "logout.html", &Empty {})
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResetQuery {
    token: Option<String>,
}

#[derive(Serialize)]
struct ResetTemplate<'s> {
    error: Option<&'s str>,
    token: Option<&'s str>,
}

/// Request a password reset or render a reset form (with a token).
///
/// ## Method
///
/// ```text
/// GET /reset
/// ```
pub fn reset(
    query: Query<ResetQuery>,
    locale: &'static Locale<'static>,
) -> RenderedTemplate {
    render(locale, "reset.html", &*query)
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ResetForm {
    CreateToken {
        email: String,
    },
    FulfilToken {
        password: String,
        password1: String,
        token: String,
    },
}

/// Send reset token in an e-mail or perform password reset (with a token).
///
/// ## Method
///
/// ```text
/// POST /reset
/// ```
pub fn do_reset(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    locale: &'static Locale<'static>,
    form: Form<ResetForm>,
) -> RenderedTemplate {
    let db = state.db.get()?;

    match form.into_inner() {
        ResetForm::CreateToken { email } => {
            let user = match User::by_email(&*db, &email) {
                Ok(user) => user,
                Err(error) => {
                    if let Some(code) = error.code() {
                        return render_code(
                            locale,
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
            let token = PasswordResetToken::create(&*db, &user)?;

            let code = token.get_code(&state.config);
            // TODO: get URL from Actix.
            let url = format!(
                "https://{}/reset?token={}", &state.config.server.domain, code);

            let user_locale = state.i18n.find_locale(&user.language())
                .unwrap_or(locale);
            Mailer::do_send(
                email.as_str(),
                "reset",
                "mail-reset-subject",
                &templates::ResetMailArgs {
                    user: user.get_public(UserFields::empty()),
                    url: &url,
                },
                user_locale,
            );

            render(locale, "reset_token_sent.html", &Empty {})
        }
        ResetForm::FulfilToken { password, password1, token: token_str } => {

            let token = match PasswordResetToken::from_code(
                &*db, &state.config, &token_str)
            {
                Ok(token) => token,
                Err(error) => {
                    if let Some(code) = error.code() {
                        return render_code(
                            locale,
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
                    locale,
                    StatusCode::BAD_REQUEST,
                    "reset.html",
                    &ResetTemplate {
                        error: Some("password:reset:passwords-dont-match"),
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
                        locale,
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

            Ok(HttpResponse::SeeOther()
                .header("Location", "/")
                .finish())
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RegisterQuery {
    invite: String,
}

#[derive(Serialize)]
struct RegisterTemplate<'s> {
    error: Option<&'s str>,
    email: &'s str,
    invite: &'s str,
    /// List of all available locales.
    locales: &'s [Locale<'s>],
    /// Locale in which the request was sent. Used as default value for locale
    /// selector.
    locale: &'s Locale<'s>,
}

/// Render registration form.
///
/// ## Method
///
/// ```text
/// GET /register
/// ```
pub fn register(
    state: actix_web::State<State>,
    locale: &'static Locale<'static>,
    query: Query<RegisterQuery>,
) -> RenderedTemplate {
    let db = state.db.get()?;
    let invite = Invite::from_code(&*db, &state.config, &query.invite)?;

    render(locale, "register.html", &RegisterTemplate {
        error: None,
        email: &invite.email,
        invite: &query.invite,
        locales: &state.i18n.locales,
        locale,
    })
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
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
pub fn do_register(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    locale: &'static Locale<'static>,
    form: Form<RegisterForm>,
) -> RenderedTemplate {
    let db = state.db.get()?;

    let requested_locale = state.i18n.find_locale(&form.language)
        .unwrap_or(locale);

    let invite = match Invite::from_code(&*db, &state.config, &form.invite) {
        Ok(invite) => invite,
        Err(error) => {
            if let Some(code) = error.code() {
                return render_code(
                    locale,
                    StatusCode::BAD_REQUEST,
                    "register.html",
                    &RegisterTemplate {
                        error: Some(code),
                        email: &form.email,
                        invite: &form.invite,
                        locales: &state.i18n.locales,
                        locale: requested_locale,
                    }
                );
            }

            return Err(error.into());
        }
    };

    if form.password != form.password1 {
        return render_code(
            locale,
            StatusCode::BAD_REQUEST,
            "register.html",
            &RegisterTemplate {
                error: Some("user:password:bad-confirmation"),
                email: &invite.email,
                invite: &form.invite,
                locales: &state.i18n.locales,
                locale: requested_locale,
            },
        );
    }

    if form.email != invite.email {
        return render_code(
            locale,
            StatusCode::BAD_REQUEST,
            "register.html",
            &RegisterTemplate {
                error: Some("user:register:email-changed"),
                email: &invite.email,
                invite: &form.invite,
                locales: &state.i18n.locales,
                locale: requested_locale,
            },
        );
    }

    let user = match invite.fulfil(
        &*db,
        &form.name,
        &form.password,
        &requested_locale.code.to_string(),
    ) {
        Ok(user) => user,
        Err(err) => {
            if err.code().is_none(){
                return Err(err.into());
            }

            return render_code(
                locale,
                err.status(),
                "register.html",
                &RegisterTemplate {
                    error: Some(err.code().unwrap()),
                    email: &form.email,
                    invite: &form.invite,
                    locales: &state.i18n.locales,
                    locale: requested_locale,
                },
            );
        }
    };

    Session::<Normal>::create(&req, &user, false);

    Ok(HttpResponse::SeeOther()
        .header("Location", "/")
        .finish())
}

/// Empty serializable structure to serve as empty context.
#[derive(Serialize)]
struct Empty {
}

type RenderedTemplate = Result<HttpResponse, Error>;

/// Render a named template with a given context.
///
/// This is a small wrapper around [`Tera::render`] which also handles errors
/// and transforms them into a usable response.
fn render<T>(locale: &'static Locale<'static>, name: &str, context: &T)
-> RenderedTemplate
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
    locale: &'static Locale<'static>, code: StatusCode, name: &str, context: &T
) -> RenderedTemplate
where
    T: serde::Serialize,
{
    use crate::templates::LocalizedTera;
    crate::templates::PAGES
        .render_i18n(name, context, locale)
        .map(|r| HttpResponse::build(code).body(r))
        .map_err(Into::into)
}
