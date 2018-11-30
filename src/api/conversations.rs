use actix_web::{App, HttpRequest, HttpResponse, http::Method};

/// Configure routes.
pub fn routes(app: App<()>) -> App<()> {
    app
        .route("/conversations/{id}", Method::GET, get_conversation)
        .route("/conversations/{id}/socket", Method::GET, get_socket)
}

/// Get a conversation.
///
/// ## Method
///
/// ```
/// GET /conversations/:id
/// ```
pub fn get_conversation(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Get a WebSocket for live updates from, and sending new messages
/// to a conversation.
///
/// ## Method
///
/// ```
/// GET /conversations/:id/socket
/// ```
pub fn get_socket(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}
