use actix::prelude::*;
use actix_web::ws::{self, CloseCode, WebsocketContext};
use std::time::Duration;

use super::protocol::{CookieGenerator, Flags, Kind, Message};

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
}

impl Actor for Client {
    type Context = WebsocketContext<Self, crate::api::State>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // TODO: register client with message broker.

        // Ping client every 30 seconds to keep connection open.
        ctx.run_interval(Duration::from_secs(30), |_, ctx| ctx.ping(""));
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        // TODO: unregister client from message broker.

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
            Some(Kind::SendMessage) => unimplemented!("send message"),
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
