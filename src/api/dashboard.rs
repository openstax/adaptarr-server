use actix_web::{App, HttpRequest, HttpResponse, http::Method};

use super::State;

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .route("/dashboard", Method::GET, get_dashboard)
}

/// Get the dashboard.
///
/// ## Method
///
/// ```
/// GET /dashboard
/// ```
pub fn get_dashboard(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}
