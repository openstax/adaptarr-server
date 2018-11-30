use actix_web::{App, HttpRequest, HttpResponse, http::Method};

/// Configure routes.
pub fn routes(app: App<()>) -> App<()> {
    app
        .route("/notifications", Method::GET, list_notifications)
        .route("/notifications/{id}", Method::POST, update_notifiation)
        .route("/events", Method::GET, event_stream)
}

/// Get list of all notifications (events) ever received by current user.
///
/// ## Method
///
/// ```
/// GET /notifications
/// ```
pub fn list_notifications(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Update a notification's state.
///
/// ## Method
///
/// ```
/// POST /notifications/:id
/// ```
pub fn update_notifiation(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Get a stream of events for current user.
///
/// ## Method
///
/// ```
/// GET /events
/// ```
pub fn event_stream(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}
