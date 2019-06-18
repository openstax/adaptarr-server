use actix::prelude::*;
use actix_web::ws::{self, CloseCode, WebsocketContext};
use std::time::Duration;

use super::{
    broker::{self, Broker, Connect, Disconnect, Event, NewMessageError},
    protocol::{
        CookieGenerator,
        Flags,
        Kind,
        Message,
        MessageInvalid,
        MessageReceived,
        NewMessage,
    },
};

/// Structure representing a remote client to the conversation protocol.
///
/// This actor is responsible for receiving messages from the client, forwarding
/// translating them, and forwarding to the conversation broker, as well
/// as doing the reverse.
pub struct Client {
    conversation: i32,
    user: i32,
    cookie: CookieGenerator,
}

impl Client {
    pub fn new(conversation: i32, user: i32) -> Self {
        Self {
            conversation,
            user,
            cookie: CookieGenerator::default(),
        }
    }

    /// Handle request for adding a new message to the conversation.
    fn send_message(&mut self, msg: Message, ctx: &mut <Self as Actor>::Context) {
        let flags = msg.flags;

        Broker::from_registry()
            .send(broker::NewMessage {
                conversation: self.conversation,
                user: self.user,
                message: msg.body.clone(),
            })
            .into_actor(self)
            .then(success_or_disconnect)
            .map(|r, _, ctx| ctx.binary(match r {
                Ok(id) => Message::build(msg.cookie, MessageReceived { id }),
                Err(NewMessageError::Validation(err)) =>
                    Message::build(msg.cookie, MessageInvalid {
                        message: Some(err.to_string()),
                    }),
            }))
            .maybe_suspend(flags, ctx);
    }
}

impl Actor for Client {
    type Context = WebsocketContext<Self, crate::api::State>;

    fn started(&mut self, ctx: &mut Self::Context) {
        Broker::from_registry()
            .send(Connect {
                conversation: self.conversation,
                user: self.user,
                addr: ctx.address().recipient(),
            })
            .into_actor(self)
            .then(success_or_disconnect)
            .map(|_, actor, ctx| {
                ctx.binary(Message::header(
                    actor.cookie.next(),
                    Kind::Connected,
                    Flags::empty(),
                ).to_bytes());
            })
            .wait(ctx);

        // Ping client every 30 seconds to keep connection open.
        ctx.run_interval(Duration::from_secs(30), |_, ctx| ctx.ping(""));
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        Broker::from_registry()
            .send(Disconnect {
                conversation: self.conversation,
                addr: ctx.address().recipient(),
            })
            .into_actor(self)
            .drop_err()
            .wait(ctx);

        Running::Stop
    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for Client {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        let msg = match msg {
            ws::Message::Binary(mut b) => b.take(),
            ws::Message::Pong(_) => return,
            ws::Message::Close(_) => return ctx.stop(),
            _ => return ctx.close(Some(CloseCode::Unsupported.into())),
        };

        let msg = match Message::parse(msg) {
            Ok(msg) => msg,
            Err(err) => return ctx.close(Some(err.close_code().into())),
        };

        if msg.cookie.is_server() {
            // We currently don't expect any responses.
            return;
        }

        match Kind::from_u16(msg.kind) {
            // Client wants to send a message.
            Some(Kind::SendMessage) => self.send_message(msg, ctx),
            // Client did not understand an event we sent them. We must handle
            // this response since it might be mandated by the event, and we
            // need to mark it as received.
            Some(Kind::UnknownEvent) => (),
            // We don't know this message type but must process it.
            None if msg.flags.contains(Flags::MUST_PROCESS) =>
                return ctx.close(Some(CloseCode::Other(4001).into())),
            // We don't know this message type and need not process it, or we
            // know this message but are not supposed to receive it
            // (e.g. Kind::Connected).
            _ => return ctx.binary(Message::header(
                msg.cookie,
                Kind::UnknownEvent,
                Flags::empty(),
            ).to_bytes()),
        };
    }
}

impl Handler<Event> for Client {
    type Result = ();

    fn handle(&mut self, ev: Event, ctx: &mut Self::Context) {
        let Event { id, user, timestamp, message, .. } = ev;

        let msg = NewMessage { id, timestamp, user, message };

        ctx.binary(Message::build(self.cookie.next(), msg));
    }
}

/// Convert an Actix mailbox error into an empty error, closing the connection
/// if it occurred.
fn success_or_disconnect<R>(
    r: Result<R, MailboxError>,
    _: &mut Client,
    ctx: &mut <Client as Actor>::Context,
) -> impl ActorFuture<Item = R, Error = (), Actor = Client> {
    match r {
        Ok(r) => actix::fut::ok(r),
        Err(e) => {
            error!("Could not deliver message to the conversation broker: {}", e);
            ctx.close(Some(CloseCode::Error.into()));
            actix::fut::err(())
        }
    }
}

trait MaybeSuspend<A: Actor> {
    fn maybe_suspend(self, flags: Flags, ctx: &mut A::Context);
}

impl<A, T> MaybeSuspend<A> for T
where
    A: Actor,
    T: ContextFutureSpawner<A>,
    <A as Actor>::Context: AsyncContext<A>,
{
    fn maybe_suspend(self, flags: Flags, ctx: &mut A::Context) {
        if flags.contains(Flags::RESPONSE_REQUIRED) {
            // Response is required for this message; suspend processing of
            // incoming messages until we handle this one.
            self.wait(ctx)
        } else {
            // Response is not required; we can safely continue accepting new
            // messages while we process this one.
            self.spawn(ctx)
        }
    }
}
