use actix_web::{App, HttpRequest, Path, HttpResponse, http::Method, ws};

use crate::models::conversation::{
    Client,
    Conversation,
    ConversationData,
    FindConversationError,
};
use super::{Error, State, session::Session};

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
    state: actix_web::State<State>,
    session: Session,
    id: Path<i32>,
) -> Result<HttpResponse, actix_web::Error> {
    let user = session.user_id();
    let conversation = find_conversation(&*state, id.into_inner(), user)?;

    ws::start(&req, Client::new(conversation.id, user))
}

fn find_conversation(state: &State, id: i32, user: i32)
-> Result<Conversation, Error> {
    let db = state.db.get()?;
    let conversation = Conversation::by_id(&*db, id)?;

    if !conversation.check_access(&*db, user)? {
        Err(FindConversationError::NotFound.into())
    } else {
        Ok(conversation)
    }
}
