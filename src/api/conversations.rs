use actix_web::{App, HttpRequest, Path, HttpResponse, Json, http::Method, ws};

use crate::{
    db::Connection,
    models::conversation::{
        Client,
        Conversation,
        ConversationData,
        FindConversationError,
    },
};
use super::{Error, State, session::Session};

type Result<T, E=Error> = std::result::Result<T, E>;

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .route("/conversations", Method::GET, list_conversations)
        .route("/conversations/{id}", Method::GET, get_conversation)
        .route("/conversations/{id}/socket", Method::GET, get_socket)
}

pub fn list_conversations(
    state: actix_web::State<State>,
    session: Session,
) -> Result<Json<Vec<ConversationData>>> {
    let db = state.db.get()?;

    Conversation::all_of(&*db, session.user_id())?
        .iter()
        .map(|c| c.get_public(&*db))
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
        .map_err(From::from)
}

/// Get a conversation.
///
/// ## Method
///
/// ```text
/// GET /conversations/:id
/// ```
pub fn get_conversation(
    state: actix_web::State<State>,
    session: Session,
    id: Path<i32>,
) -> Result<Json<ConversationData>> {
    let db = state.db.get()?;

    find_conversation(&*db, id.into_inner(), session.user_id())?
        .get_public(&*db)
        .map(Json)
        .map_err(From::from)
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
    let db = state.db.get().map_err(Error::from)?;
    let user = session.user_id();
    let conversation = find_conversation(&*db, id.into_inner(), user)?;

    ws::start(&req, Client::new(conversation.id, user))
}

fn find_conversation(db: &Connection, id: i32, user: i32)
-> Result<Conversation, Error> {
    let conversation = Conversation::by_id(db, id)?;

    if !conversation.check_access(db, user)? {
        Err(FindConversationError::NotFound.into())
    } else {
        Ok(conversation)
    }
}
