use actix_web::{HttpRequest, HttpResponse, web::{self, ServiceConfig}};

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .route("/conversations/{id}", web::get().to(get_conversation))
        .route("/conversations/{id}/socket", web::get().to(get_socket));
}

/// Get a conversation.
///
/// ## Method
///
/// ```text
/// GET /conversations/:id
/// ```
fn get_conversation(_req: HttpRequest) -> HttpResponse {
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
fn get_socket(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}
