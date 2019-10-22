use actix_web::{HttpRequest, HttpResponse, http::StatusCode, web::{self, Json, ServiceConfig, Path}};
use adaptarr_conversations::Broker;
use adaptarr_error::{ApiError, Error};
use adaptarr_models::{Ticket, Model};
use adaptarr_web::{Created, Database, FormOrJson, Session};
use diesel::Connection as _;
use failure::Fail;
use serde::Deserialize;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .service(web::resource("/support/tickets")
            .route(web::get().to(list_tickets))
            .route(web::post().to(open_ticket))
        )
        .route("/support/tickets/my", web::get().to(list_my_tickets))
        .service(web::resource("/support/tickets/{id}")
            .name("support-ticket")
            .route(web::get().to(get_ticket))
            .route(web::put().to(update_ticket))
        )
        .route("/support/tickets/{id}/join", web::post().to(join_ticket))
    ;
}

/// Get list of all tickets.
///
/// Users who are not part of the support team will only receive ticket they
/// opened.
///
/// ## Method
///
/// ```text
/// GET /support/tickets
/// ```
fn list_tickets(db: Database, session: Session)
-> Result<Json<Vec<<Ticket as Model>::Public>>> {
    let user = session.user(&db)?;

    let tickets = if user.is_support {
        Ticket::all(&db)?
    } else {
        Ticket::all_of(&db, &user)?
    };

    Ok(Json(tickets.get_public_full(&db, &())?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewTicket {
    title: String,
}

/// Open a new ticket.
///
/// ## Method
///
/// ```text
/// POST /support/my/tickets
/// ```
fn open_ticket(
    req: HttpRequest,
    db: Database,
    session: Session,
    data: FormOrJson<NewTicket>,
) -> Result<Created<String, Json<<Ticket as Model>::Public>>> {
    let user = session.user(&db)?;
    let data = data.into_inner();
    let ticket = Ticket::create(&db, &data.title, &user)?;

    let location = req.url_for("support-ticket", &[ticket.id().to_string()])?;
    Ok(Created(location.to_string(), Json(ticket.get_public_full(&db, &())?)))
}

/// Get list of all tickets opened by current user.
///
/// ## Method
///
/// ```text
/// GET /support/my/tickets
/// ```
fn list_my_tickets(db: Database, session: Session)
-> Result<Json<Vec<<Ticket as Model>::Public>>> {
    Ok(Json(Ticket::all_of(&db, &session.user(&db)?)?.get_public_full(&db, &())?))
}

/// Get details of a specific ticket.
///
/// ## Method
///
/// ```text
/// GET /support/tickets/:id
/// ```
fn get_ticket(db: Database, session: Session, id: Path<i32>)
-> Result<Json<<Ticket as Model>::Public>> {
    let user = session.user(&db)?;

    let ticket = if user.is_support {
        Ticket::by_id(&db, *id)?
    } else {
        Ticket::by_id_and_user(&db, *id, &user)?
    };

    Ok(Json(ticket.get_public_full(&db, &())?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TicketUpdate {
    #[serde(default)]
    title: Option<String>,
}

/// Update an ticket.
///
/// ## Method
///
/// ```text
/// PUT /support/tickets/:id
/// ```
fn update_ticket(
    db: Database,
    session: Session,
    id: Path<i32>,
    data: FormOrJson<TicketUpdate>,
) -> Result<Json<<Ticket as Model>::Public>> {
    let user = session.user(&db)?;

    if !user.is_support {
        return Err(NotSupport.into());
    }

    let data = data.into_inner();
    let mut ticket = Ticket::by_id(&db, id.into_inner())?;

    db.transaction::<_, Error, _>(|| {
        if let Some(title) = data.title {
            ticket.set_title(&db, &title)?;
        }

        Ok(())
    })?;

    Ok(Json(ticket.get_public_full(&db, &())?))
}

#[derive(ApiError, Debug, Fail)]
#[api(status = "NOT_FOUND")]
#[fail(display = "user is not part of the support group")]
struct NotSupport;

/// Join conversation associated with this ticket.
///
/// ## Method
///
/// ```text
/// POST /support/tickets/:id/join
/// ```
fn join_ticket(db: Database, session: Session, id: Path<i32>)
-> Result<HttpResponse> {
    let user = session.user(&db)?;

    if !user.is_support {
        return Err(NotSupport.into());
    }

    let mut conversation = Ticket::by_id(&db, id.into_inner())?
        .conversation(&db)?;
    let users = [user];

    if let Some(ev) = conversation.add_members(&db, &users)? {
        Broker::dispatch(conversation.id(), ev);
    }

    Ok(HttpResponse::new(StatusCode::OK))
}
