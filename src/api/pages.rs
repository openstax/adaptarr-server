use actix_web::{
    App,
    Form,
    HttpRequest,
    HttpResponse,
    Query,
    error::{Error, ErrorInternalServerError},
    http::{Method, StatusCode},
    middleware::Logger,
};

use crate::models::{
    Invite,
    PasswordResetToken,
    user::{User, PublicData as UserData},
};
use super::{
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
            r.get().with(login);
            r.post().with(do_login);
        })
        .resource("/elevate", |r| {
            r.get().with(elevate);
            r.post().with(do_elevate);
        })
        .route("/logout", Method::GET, logout)
        .resource("/reset", |r| {
            r.get().with(reset);
            r.post().with(do_reset);
        })
        .resource("/register", |r| {
            r.name("register");
            r.get().with(register);
            r.post().with(do_register);
        })
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginQuery {
    next: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoginTemplate {
    error: Option<String>,
    next: Option<String>,
}

/// Render a login screen.
///
/// ## Method
///
/// ```
/// GET /login
/// ```
pub fn login((
    session,
    query,
): (
    Option<Session>,
    Query<LoginQuery>,
)) -> RenderedTemplate {
    if let Some(_) = session {
        return Ok(HttpResponse::SeeOther()
            .header("Location", query.next.as_ref().map_or("/", String::as_str))
            .finish());
    }

    render("login.html", &LoginTemplate {
        error: None,
        next: query.into_inner().next,
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
pub fn do_login((
    req,
    params,
): (
    HttpRequest<State>,
    Form<LoginCredentials>,
)) -> RenderedTemplate {
    let db = &*req.state().db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    let user = match User::authenticate(db, &params.email, &params.password) {
        Ok(user) => user,
        Err(ref err) if err.is_internal() => {
            return Err(ErrorInternalServerError(err.to_string()));
        }
        Err(err) => return render_code(
            StatusCode::BAD_REQUEST,
            "login.html",
            &LoginTemplate {
                error: Some(err.to_string()),
                next: params.into_inner().next,
            },
        ),
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
pub fn elevate((
    state,
    session,
    query,
): (
    actix_web::State<State>,
    Session,
    Query<LoginQuery>,
)) -> RenderedTemplate {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let user = User::by_id(&*db, session.user)?;

    if !user.is_super {
        return Ok(HttpResponse::Forbidden().finish());
    }

    render("elevate.html", &LoginTemplate {
        error: None,
        next: query.into_inner().next,
    })
}

#[derive(Debug, Deserialize)]
pub struct ElevateCredentials {
    password: String,
    next: Option<String>,
}

/// Perform session elevation.
///
/// ## Method
///
/// ```
/// POST /elevate
/// ```
pub fn do_elevate((
    req,
    state,
    session,
    form,
): (
    HttpRequest<State>,
    actix_web::State<State>,
    Session,
    Form<ElevateCredentials>,
)) -> RenderedTemplate {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let user = User::by_id(&*db, session.user)?;

    if !user.is_super {
        return Ok(HttpResponse::Forbidden().finish());
    }

    if !user.check_password(&form.password) {
        return render_code(
            StatusCode::BAD_REQUEST,
            "elevate.html",
            &LoginTemplate {
                error: Some("Bad password".to_string()),
                next: form.into_inner().next,
            },
        );
    }

    Session::<Normal>::create(&req, user.id, true);

    Ok(HttpResponse::SeeOther()
        .header("Location", form.next.as_ref().map_or("/", String::as_str))
        .finish())
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
pub fn do_reset((
    req,
    state,
    form,
): (
    HttpRequest<State>,
    actix_web::State<State>,
    Form<ResetForm>,
))
-> RenderedTemplate {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    match form.into_inner() {
        ResetForm::CreateToken { email } => {
            let user = User::by_email(&*db, &email)?;
            let token = PasswordResetToken::create(&*db, &user)
                .map_err(|e| ErrorInternalServerError(e.to_string()))?;

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

            let user = token.fulfil(&*db, &password)
                .map_err(|e| ErrorInternalServerError(e.to_string()))?;
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
pub fn register((
    state,
    query,
): (
    actix_web::State<State>,
    Query<RegisterQuery>,
)) -> RenderedTemplate {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
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
pub fn do_register((
    req,
    state,
    form,
): (
    HttpRequest<State>,
    actix_web::State<State>,
    Form<RegisterForm>,
)) -> RenderedTemplate {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
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

    let user = invite.fulfil(&*db, &form.name, &form.password)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

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
        .map_err(|e| ErrorInternalServerError(e.to_string()))
}
