use actix_web::{
    App,
    HttpRequest,
    HttpResponse,
    http::Method,
    middleware::Logger,
};

pub fn app() -> App {
    App::new()
        .middleware(Logger::default())
        .resource("/login", |r| {
            r.get().f(login);
            r.post().f(do_login);
        })
        .route("/logout", Method::GET, logout)
        .resource("/reset", |r| {
            r.get().f(reset);
            r.post().f(do_reset);
        })
        .resource("/register", |r| {
            r.get().f(register);
            r.post().f(do_register);
        })
}

/// Render a login screen.
///
/// ## Method
///
/// ```
/// GET /login
/// ```
pub fn login(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Perform login.
///
/// ## Method
///
/// ```
/// POST /login
/// ```
pub fn do_login(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Log an user out and destroy their session.
///
/// ## Method
///
/// ```
/// GET /logout
/// ```
pub fn logout(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Request a password reset or render a reset form (with a token).
///
/// ## Method
///
/// ```
/// GET /reset
/// ```
pub fn reset(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Send reset token in an e-mail or perform password reset (with a token).
///
/// ## Method
///
/// ```
/// POST /reset
/// ```
pub fn do_reset(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Render registration form.
///
/// ## Method
///
/// ```
/// GET /register
/// ```
pub fn register(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Perform registration.
///
/// ## Method
///
/// ```
/// POST /register
/// ```
pub fn do_register(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}
