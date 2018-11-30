use actix_web::{App, HttpRequest, HttpResponse, http::Method};

/// Configure routes.
pub fn routes(app: App<()>) -> App<()> {
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
pub fn get_dashboard(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}
