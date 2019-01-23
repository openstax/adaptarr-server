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

use crate::models::{
    Invite,
    PasswordResetToken,
    user::{User, PublicData as UserData},
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    session::{SessionManager, Session, Normal},
};

pub fn app(state: State) -> App<State> {
    let sessions = SessionManager::new(
        state.config.server.secret.clone(),
        state.db.clone(),
    );

    App::with_state(state)
        .middleware(Logger::default())
        .middleware(sessions)
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

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginQuery {
    next: Option<String>,
    #[serde(default)]
    action: LoginAction,
}

#[derive(Debug, Serialize)]
struct LoginTemplate {
    error: Option<String>,
    next: Option<String>,
    action: LoginAction,
}

/// Render a login screen.
///
/// ## Method
///
/// ```
/// GET /login
/// ```
pub fn login(
    session: Option<Session>,
    query: Query<LoginQuery>,
) -> RenderedTemplate {
    if let Some(_) = session {
        return Ok(HttpResponse::SeeOther()
            .header("Location", query.next.as_ref().map_or("/", String::as_str))
            .finish());
    }

    let LoginQuery { next, action } = query.into_inner();

    render("login.html", &LoginTemplate {
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
/// ```
/// POST /login
/// ```
pub fn do_login(
    req: HttpRequest<State>,
    params: Form<LoginCredentials>,
) -> RenderedTemplate {
    let db = &*req.state().db.get()?;

    let user = match User::authenticate(db, &params.email, &params.password) {
        Ok(user) => user,
        Err(err) => {
            if err.is_internal() {
                return Err(err.into());
            }
            return render_code(
                StatusCode::BAD_REQUEST,
                "login.html",
                &LoginTemplate {
                    error: Some(err.to_string()),
                    next: params.into_inner().next,
                    action: LoginAction::default(),
                },
            );
        }
    };

    // NOTE: This will automatically remove any session that may still exist,
    // we don't have to do it manually here.
    Session::<Normal>::create(&req, user.id, false);

    Ok(HttpResponse::SeeOther()
        .header("Location", params.next.as_ref().map_or("/", String::as_str))
        .finish())
}

/// Render a session elevation screen.
///
/// ## Method
///
/// ```
/// GET /elevate
/// ```
pub fn elevate(
    state: actix_web::State<State>,
    session: Session,
    query: Query<LoginQuery>,
) -> RenderedTemplate {
    let db = state.db.get()?;
    let user = User::by_id(&*db, session.user)?;
    let LoginQuery { next, action } = query.into_inner();

    if !user.is_super {
        return Ok(HttpResponse::Forbidden().finish());
    }

    render("elevate.html", &LoginTemplate {
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
/// ```
/// POST /elevate
/// ```
pub fn do_elevate(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    session: Session,
    form: Form<ElevateCredentials>,
) -> RenderedTemplate {
    let db = state.db.get()?;
    let user = User::by_id(&*db, session.user)?;
    let ElevateCredentials { next, action, password } = form.into_inner();

    if !user.is_super {
        return Ok(HttpResponse::Forbidden().finish());
    }

    if !user.check_password(&password) {
        return render_code(
            StatusCode::BAD_REQUEST,
            "elevate.html",
            &LoginTemplate {
                error: Some("Bad password".to_string()),
                next,
                action,
            },
        );
    }

    Session::<Normal>::create(&req, user.id, true);

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
/// ```
/// POST /elevate
/// Accept: application/json
/// ```
pub fn do_elevate_json(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    session: Session,
    form: Form<ElevateCredentials>,
) -> RenderedTemplate {
    let db = state.db.get()?;
    let user = User::by_id(&*db, session.user)?;
    let ElevateCredentials { password, .. } = form.into_inner();

    if !user.is_super {
        return Ok(HttpResponse::Forbidden().finish());
    }

    if !user.check_password(&password) {
        return Ok(HttpResponse::BadRequest()
            .json(ElevationResult::Error {
                message: "Bad password".to_string(),
            }));
    }

    Session::<Normal>::create(&req, user.id, true);

    Ok(HttpResponse::Ok().json(ElevationResult::Success))
}

/// Log an user out and destroy their session.
///
/// ## Method
///
/// ```
/// GET /logout
/// ```
pub fn logout((req, sess): (HttpRequest<State>, Session)) -> RenderedTemplate {
    Session::destroy(&req, sess);
    render("logout.html", &Empty {})
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
/// ```
/// GET /reset
/// ```
pub fn reset(query: Query<ResetQuery>) -> RenderedTemplate {
    render("reset.html", &*query)
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

#[derive(Serialize)]
struct ResetMail<'s> {
    user: UserData,
    url: &'s str,
}

/// Send reset token in an e-mail or perform password reset (with a token).
///
/// ## Method
///
/// ```
/// POST /reset
/// ```
pub fn do_reset(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    form: Form<ResetForm>,
) -> RenderedTemplate {
    let db = state.db.get()?;

    match form.into_inner() {
        ResetForm::CreateToken { email } => {
            let user = User::by_email(&*db, &email)?;
            let token = PasswordResetToken::create(&*db, &user)?;

            let code = token.get_code(&state.config);
            // TODO: get URL from Actix.
            let url = format!(
                "https://{}/reset?token={}", &state.config.server.domain, code);

            state.mailer.send(
                "reset", email.as_str(), "Password reset", &ResetMail {
                    user: user.get_public(),
                    url: &url,
                });

            render("reset_token_sent.html", &Empty {})
        }
        ResetForm::FulfilToken { password, password1, token: token_str } => {
            let token = PasswordResetToken::from_code(
                &*db, &state.config, &token_str)?;

            if password != password1 {
                return render_code(StatusCode::BAD_REQUEST, "reset.html", &ResetTemplate {
                    error: Some("Passwords don't match"),
                    token: Some(&token_str),
                });
            }

            let user = token.fulfil(&*db, &password)?;
            Session::<Normal>::create(&req, user.id, false);

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
}

/// Render registration form.
///
/// ## Method
///
/// ```
/// GET /register
/// ```
pub fn register(
    state: actix_web::State<State>,
    query: Query<RegisterQuery>,
) -> RenderedTemplate {
    let db = state.db.get()?;
    let invite = Invite::from_code(&*db, &state.config, &query.invite)?;

    render("register.html", &RegisterTemplate {
        error: None,
        email: &invite.email,
        invite: &query.invite,
    })
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    email: String,
    name: String,
    password: String,
    password1: String,
    invite: String,
}

/// Perform registration.
///
/// ## Method
///
/// ```
/// POST /register
/// ```
pub fn do_register(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    form: Form<RegisterForm>,
) -> RenderedTemplate {
    let db = state.db.get()?;
    let invite = Invite::from_code(&*db, &state.config, &form.invite)?;

    if form.password != form.password1 {
        return render_code(StatusCode::BAD_REQUEST, "register.html", &RegisterTemplate {
            error: Some("Passwords don't match"),
            email: &invite.email,
            invite: &form.invite,
        });
    }

    if form.email != invite.email {
        return render_code(StatusCode::BAD_REQUEST, "register.html", &RegisterTemplate {
            error: Some("You can't change your email during registration"),
            email: &invite.email,
            invite: &form.invite,
        });
    }

    let user = invite.fulfil(&*db, &form.name, &form.password)?;

    Session::<Normal>::create(&req, user.id, false);

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
fn render<T>(name: &str, context: &T) -> RenderedTemplate
where
    T: serde::Serialize,
{
    render_code(StatusCode::OK, name, context)
}

/// Render a named template with a given context and given status code.
///
/// This is a small wrapper around [`Tera::render`] which also handles errors
/// and transforms them into a usable response.
fn render_code<T>(code: StatusCode, name: &str, context: &T) -> RenderedTemplate
where
    T: serde::Serialize,
{
    crate::templates::PAGES
        .render(name, context)
        .map(|r| HttpResponse::build(code).body(r))
        .map_err(Into::into)
}
