use actix_web::{App, HttpRequest, HttpResponse, http::Method};

use super::State;

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app.scope("/users", |scope| scope
        .route("/invite", Method::POST, create_invitation)
        .resource("/{id}", |r| {
            r.get().f(get_user);
            r.put().f(modify_user);
        })
        .route("/me/password", Method::PUT, modify_password))
}

/// Create an invitation.
///
/// ## Method
///
/// ```
/// POST /users/invite
/// ```
pub fn create_invitation(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Get user information.
///
/// ## Method
///
/// ```
/// GET /users/:id
/// ```
pub fn get_user(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Update user information.
///
/// ## Method
///
/// ```
/// PUT /users/:id
/// ```
pub fn modify_user(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Change password.
///
/// ## Method
///
/// ```
/// PUT /users/me/password
/// ```
pub fn modify_password(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}
