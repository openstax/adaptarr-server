use actix::{
    Actor,
    Running,
    StreamHandler,
    Handler,
    SystemService,
    AsyncContext,
    WrapFuture,
    ActorFuture,
    ContextFutureSpawner,
};
use actix_web::{
    HttpRequest,
    HttpResponse,
    http::StatusCode,
    web::{self, Payload, Path, Json, ServiceConfig},
};
use actix_web_actors::ws::{self, WebsocketContext};
use adaptarr_models::{Event, Model, events};
use adaptarr_web::{Database, Session};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .route("/notifications", web::get().to(list_notifications))
        .route("/notifications/{id}", web::put().to(update_notifiation))
        .route("/events", web::get().to(event_stream))
    ;
}

#[derive(Serialize)]
struct EventData {
    id: i32,
    kind: &'static str,
    timestamp: DateTime<Utc>,
    #[serde(flatten)]
    data: events::Event,
}

/// Get list of all notifications (events) ever received by current user.
///
/// ## Method
///
/// ```text
/// GET /notifications
/// ```
fn list_notifications(db: Database, session: Session)
-> Result<Json<Vec<EventData>>> {
    let events = Event::unread(&db, session.user)?
        .into_iter()
        .map(|event| {
            let data = event.get_public();

            EventData {
                id: event.id,
                kind: data.kind(),
                timestamp: event.timestamp,
                data,
            }
        })
        .collect();

    Ok(Json(events))
}

#[derive(Deserialize)]
struct EventUpdate {
    unread: bool,
}

/// Update a notification's state.
///
/// ## Method
///
/// ```text
/// PUT /notifications/:id
/// ```
fn update_notifiation(
    db: Database,
    session: Session,
    id: Path<i32>,
    update: Json<EventUpdate>,
) -> Result<HttpResponse> {
    Event::by_id(&db, (*id, session.user))?.set_unread(&*db, update.unread)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

/// Get a stream of events for current user.
///
/// Note that this probably should be an SSE stream instead, since we only emit
/// messages.
///
/// ## Method
///
/// ```text
/// GET /events
/// ```
fn event_stream(req: HttpRequest, session: Session, stream: Payload)
-> Result<HttpResponse, actix_web::error::Error> {
    ws::start(Listener { user: session.user }, &req, stream)
}

/// Stream of events.
struct Listener {
    user: i32,
}

impl Actor for Listener {
    type Context = WebsocketContext<Self>;

    /// Register this stream as an event listener.
    fn started(&mut self, ctx: &mut Self::Context) {


        events::EventManager::from_registry()
            .send(events::RegisterListener {
                user: self.user,
                addr: ctx.address().recipient(),
            })
            .into_actor(self)
            .then(|_, _, _| actix::fut::ok(()))
            .wait(ctx);

        // Ping client every 30 seconds to keep connection open.
        ctx.run_interval(Duration::from_secs(30), |_, ctx| ctx.ping(""));
    }

    /// Unregister as an event listener.
    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        events::EventManager::from_registry()
            .do_send(events::UnregisterListener {
                user: self.user,
                addr: ctx.address().recipient(),
            });
        Running::Stop
    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for Listener {
    fn handle(&mut self, _: ws::Message, _: &mut Self::Context) {
        // We don't consume messages.
    }
}

impl Handler<events::NewEvent> for Listener {
    type Result = ();

    fn handle(&mut self, msg: events::NewEvent, ctx: &mut Self::Context) {
        let events::NewEvent { id, timestamp, event } = msg;
        ctx.text(serde_json::to_string(&EventData {
            id,
            kind: event.kind(),
            timestamp,
            data: (*event).clone(),
        }).unwrap());
    }
}
