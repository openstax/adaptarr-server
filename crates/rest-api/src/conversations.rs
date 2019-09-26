use actix_web::{
    HttpRequest,
    HttpResponse,
    web::{self, Payload, Path, Json, ServiceConfig},
};
use adaptarr_conversations::Client;
use adaptarr_models::{
    db::Connection,
    models::{
        FindModelError,
        Model,
        conversation::Conversation,
    }
};
use adaptarr_web::{Database, Session};
use actix_web_actors::ws;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .service(web::resource("/conversations")
            .route(web::get().to(list_conversations))
        )
        .service(web::resource("/conversations/{id}")
            .route(web::get().to(get_conversation))
        )
        .service(web::resource("/conversations/{id}/socket")
            .route(web::get().to(get_socket))
        )
    ;
}

pub fn list_conversations(db: Database, session: Session) -> Result<Json<Vec<<Conversation as Model>::Public>>> {
    Ok(Json(Conversation::all_of(&db, session.user_id())?
        .get_public_full(&db, &())?))
}

/// Get a conversation.
///
/// ## Method
///
/// ```text
/// GET /conversations/:id
/// ```
pub fn get_conversation(
    db: Database,
    session: Session,
    id: Path<i32>,
) -> Result<Json<<Conversation as Model>::Public>> {
    Ok(Json(find_conversation(&db, id.into_inner(), session.user_id())?
        .get_public_full(&db, &())?))
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
    db: Database,
    req: HttpRequest,
    session: Session,
    id: Path<i32>,
    stream: Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let user = session.user_id();
    let conversation = find_conversation(&db, id.into_inner(), user)?;

    ws::start(Client::new(conversation.id, user), &req, stream)
}

fn find_conversation(db: &Connection, id: i32, user: i32)
-> Result<Conversation> {
    let conversation = Conversation::by_id(db, id)?;

    if !conversation.check_access(db, user)? {
        Err(FindModelError::<Conversation>::not_found().into())
    } else {
        Ok(conversation)
    }
}
