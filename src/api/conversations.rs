use actix_web::{App, HttpRequest, HttpResponse, http::Method};

use super::State;

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .route("/conversations/{id}", Method::GET, get_conversation)
        .route("/conversations/{id}/socket", Method::GET, get_socket)
}

/// Get a conversation.
///
/// ## Method
///
/// ```text
/// GET /conversations/:id
/// ```
pub fn get_conversation(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Get a WebSocket for live updates from, and sending new messages
/// to a conversation.
///
/// ## Method
///
/// ```text
/// GET /conversations/:id/socket
/// ```
pub fn get_socket(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}
