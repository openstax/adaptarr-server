use actix_web::{App, HttpRequest, HttpResponse, http::Method};

use super::State;

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/bookparts/{id}", |r| {
            r.get().f(get_part);
            r.delete().f(delete_part);
        })
        .route("/bookparts/{id}/parts", Method::POST, insert_into_part)
        .route("/bookparts/{id}/move", Method::POST, move_part)
}

/// Get a book part.
///
/// ## Method
///
/// ```
/// GET /bookparts/:id
/// ```
pub fn get_part(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Delete a book part.
///
/// ## Method
///
/// ```
/// DELETE /bookparts/:id
/// ```
pub fn delete_part(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Create a new element in a book part.
///
/// ## Method
///
/// ```
/// POST /bookparts/:id/parts
/// ```
pub fn insert_into_part(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Move part to a different location.
///
/// ## Method
///
/// ```
/// POST /bookparts/:id/move
/// ```
pub fn move_part(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}
