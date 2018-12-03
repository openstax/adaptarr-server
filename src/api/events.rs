use actix_web::{App, HttpRequest, HttpResponse, http::Method};

use super::State;

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
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
pub fn list_notifications(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Update a notification's state.
///
/// ## Method
///
/// ```
/// POST /notifications/:id
/// ```
pub fn update_notifiation(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Get a stream of events for current user.
///
/// ## Method
///
/// ```
/// GET /events
/// ```
pub fn event_stream(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}
