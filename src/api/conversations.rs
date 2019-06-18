use actix_web::{App, HttpRequest, Path, HttpResponse, http::Method, ws};

use crate::models::conversation::Client;
use super::{State, session::Session};

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
pub fn get_socket(
    req: HttpRequest<State>,
    session: Session,
    id: Path<i32>,
) -> Result<HttpResponse, actix_web::Error> {
    let conversation = id.into_inner();
    let user = session.user_id();

    ws::start(&req, Client::new(conversation, user))
}
