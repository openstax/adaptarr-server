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
use tera::Tera;

use crate::models::{Invite, User};
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
        .route("/logout", Method::GET, logout)
        .resource("/reset", |r| {
            r.get().f(reset);
            r.post().f(do_reset);
        })
        .resource("/register", |r| {
            r.get().with(register);
            r.post().with(do_register);
        })
}

lazy_static! {
    static ref TERA: Tera = {
        compile_templates!("templates/**/*")
    };
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

/// Request a password reset or render a reset form (with a token).
///
/// ## Method
///
/// ```
/// GET /reset
/// ```
pub fn reset(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Send reset token in an e-mail or perform password reset (with a token).
///
/// ## Method
///
/// ```
/// POST /reset
/// ```
pub fn do_reset(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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
    TERA.render(name, context)
        .map(|r| HttpResponse::build(code).body(r))
        .map_err(|e| ErrorInternalServerError(e.to_string()))
}
