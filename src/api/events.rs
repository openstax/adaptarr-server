use actix::{
    Actor,
    ActorFuture,
    AsyncContext,
    ContextFutureSpawner,
    Handler,
    Running,
    StreamHandler,
    SystemService,
    WrapFuture,
};
use actix_web::{
    App,
    FromRequest,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    http::{StatusCode, Method},
    ws::{self, WebsocketContext},
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{
    events,
    models::Event,
};
use super::{
    Error,
    State,
    RouterExt,
    session::{Session, Normal},
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .api_route("/notifications", Method::GET, list_notifications)
        .api_route("/notifications/{id}", Method::PUT, update_notifiation)
        .route("/events", Method::GET, event_stream)
}

type Result<T, E=Error> = std::result::Result<T, E>;

#[derive(Debug, Serialize)]
pub struct EventData {
    id: i32,
    kind: &'static str,
    timestamp: NaiveDateTime,
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
pub fn list_notifications(
    state: actix_web::State<State>,
    session: Session,
) -> Result<Json<Vec<EventData>>> {
    let db = state.db.get()?;
    let events = Event::unread(&*db, session.user)?
        .into_iter()
        .map(|event| {
            let data = event.load();

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

#[derive(Debug, Deserialize)]
pub struct EventUpdate {
    unread: bool,
}

/// Update a notification's state.
///
/// ## Method
///
/// ```text
/// PUT /notifications/:id
/// ```
pub fn update_notifiation(
    state: actix_web::State<State>,
    session: Session,
    id: Path<i32>,
    update: Json<EventUpdate>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let mut event = Event::by_id(&*db, *id, session.user)?;

    event.set_unread(&*db, update.unread)?;

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
pub fn event_stream(
    req: HttpRequest<State>,
    _session: Session,
) -> Result<HttpResponse, actix_web::error::Error> {
    ws::start(&req, Listener)
}

/// Stream of events.
struct Listener;

impl Actor for Listener {
    type Context = WebsocketContext<Self, State>;

    /// Register this stream as an event listener.
    fn started(&mut self, ctx: &mut Self::Context) {
        let session = Session::<Normal>::extract(ctx.request()).unwrap();

        events::EventManager::from_registry()
            .send(events::RegisterListener {
                user: session.user,
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
        let session = Session::<Normal>::extract(ctx.request()).unwrap();

        events::EventManager::from_registry()
            .do_send(events::UnregisterListener {
                user: session.user,
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
        ctx.text(serde_json::to_vec(&EventData {
            id,
            kind: event.kind(),
            timestamp,
            data: (*event).clone(),
        }).unwrap());
    }
}
