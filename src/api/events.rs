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
    http::Method,
    ws::{self, WebsocketContext},
};
use chrono::NaiveDateTime;

use crate::events;
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

/// Get list of all notifications (events) ever received by current user.
///
/// ## Method
///
/// ```
/// GET /notifications
/// ```
pub fn list_notifications(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Update a notification's state.
///
/// ## Method
///
/// ```
/// POST /notifications/:id
/// ```
pub fn update_notifiation(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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
)) -> Result<HttpResponse, actix_web::Error> {
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

#[derive(Debug, Serialize)]
struct Event<'a> {
    id: i32,
    kind: &'a str,
    timestamp: NaiveDateTime,
    #[serde(flatten)]
    data: events::Event,
}

impl Handler<events::NewEvent> for Listener {
    type Result = ();

    fn handle(&mut self, msg: events::NewEvent, ctx: &mut Self::Context) {
        let events::NewEvent { id, timestamp, event } = msg;
        ctx.binary(serde_json::to_vec(&Event {
            id,
            kind: event.kind(),
            timestamp,
            data: event,
        }).unwrap());
    }
}
