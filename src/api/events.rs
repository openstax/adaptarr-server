use actix::{
    Actor,
    ActorFuture,
    AsyncContext,
    ContextFutureSpawner,
    Handler,
    Running,
    StreamHandler,
    WrapFuture,
};
use actix_web::{
    App,
    FromRequest,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    error::ErrorInternalServerError,
    http::Method,
    ws::{self, WebsocketContext},
};
use chrono::NaiveDateTime;

use crate::{
    events,
    models::Event,
};
use super::{
    State,
    session::{Session, Normal},
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .route("/notifications", Method::GET, list_notifications)
        .route("/notifications/{id}", Method::POST, update_notifiation)
        .route("/events", Method::GET, event_stream)
}

type Result<T> = std::result::Result<T, actix_web::Error>;

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
/// ```
/// GET /notifications
/// ```
pub fn list_notifications((
    state,
    session,
): (
    actix_web::State<State>,
    Session,
)) -> Result<Json<Vec<EventData>>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let events = Event::unread(&*db, session.user)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?
        .into_iter()
        .map(|event| {
            let data = event.load();

            EventData {
                id: event.id,
                kind: data.kind(),
                timestamp: event.timestamp,
                data: data,
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
/// ```
/// POST /notifications/:id
/// ```
pub fn update_notifiation((
    state,
    session,
    id,
    update,
): (
    actix_web::State<State>,
    Session,
    Path<i32>,
    Json<EventUpdate>,
)) -> Result<HttpResponse> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let mut event = Event::by_id(&*db, *id, session.user)?;

    event.set_unread(&*db, update.unread)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(HttpResponse::Ok().finish())
}

/// Get a stream of events for current user.
///
/// Note that this probably should be an SSE stream instead, since we only emit
/// messages.
///
/// ## Method
///
/// ```
/// GET /events
/// ```
pub fn event_stream((
    req,
    _session,
): (
    HttpRequest<State>,
    Session,
)) -> Result<HttpResponse> {
    ws::start(&req, Listener)
}

/// Stream of events.
struct Listener;

impl Actor for Listener {
    type Context = WebsocketContext<Self, State>;

    /// Register this stream as an event listener.
    fn started(&mut self, ctx: &mut Self::Context) {
        let session = Session::<Normal>::extract(ctx.request()).unwrap();

        ctx.state()
            .events
            .send(events::RegisterListener {
                user: session.user,
                addr: ctx.address().recipient(),
            })
            .into_actor(self)
            .then(|_, _, _| actix::fut::ok(()))
            .wait(ctx);
    }

    /// Unregister as an event listener.
    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        let session = Session::<Normal>::extract(ctx.request()).unwrap();

        ctx.state()
            .events
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
        ctx.binary(serde_json::to_vec(&EventData {
            id,
            kind: event.kind(),
            timestamp,
            data: event,
        }).unwrap());
    }
}
