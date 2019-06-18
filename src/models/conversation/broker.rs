use actix::prelude::*;
use bytes::Bytes;
use chrono::{Utc, NaiveDateTime};
use failure::Fail;
use std::collections::hash_map::{Entry, HashMap};

use crate::models::conversation::format::{self, ValidationError};

/// Broker messages and events to users.
#[derive(Default)]
pub struct Broker {
    /// Mapping from conversation ID to a list of listeners for that
    /// conversation.
    listeners: HashMap<i32, Vec<Listener>>,
}

struct Listener {
    /// User for which this listener is registered.
    user: i32,
    /// Connection to the listener.
    addr: Recipient<Event>,
}

impl Actor for Broker {
    type Context = Context<Self>;
}

impl Supervised for Broker {
}

impl SystemService for Broker {
}

/// A client connects to the broker.
pub struct Connect {
    /// User connecting.
    pub user: i32,
    /// Conversation which the user is joining.
    pub conversation: i32,
    /// Connection to the new listener.
    pub addr: Recipient<Event>,
}

impl Message for Connect {
    type Result = ();
}

impl Handler<Connect> for Broker {
    type Result = ();

    fn handle(&mut self, msg: Connect, _: &mut Self::Context) {
        let Connect { user, conversation, addr } = msg;

        // TODO: verify the user can access this conversation

        self.listeners.entry(conversation)
            .or_default()
            .push(Listener { user, addr });
    }
}

/// A client disconnects from the broker.
pub struct Disconnect {
    /// Conversation which the user is leaving.
    pub conversation: i32,
    /// Connection to the listener.
    pub addr: Recipient<Event>,
}

impl Message for Disconnect {
    type Result = ();
}

impl Handler<Disconnect> for Broker {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Self::Context) {
        let Disconnect { conversation, addr } = msg;

        if let Entry::Occupied(mut entry) = self.listeners.entry(conversation) {
            entry.get_mut().retain(|l| l.addr != addr);

            if entry.get().is_empty() {
                entry.remove();
            }
        }
    }
}

/// Message sent by a user.
pub struct NewMessage {
    /// Conversation to which this message was sent.
    pub conversation: i32,
    /// User who sent this message.
    pub user: i32,
    /// Message body.
    pub message: Bytes,
}

#[derive(Debug, Fail)]
pub enum NewMessageError {
    #[fail(display = "malformed message: {}", _0)]
    Validation(#[cause] ValidationError),
}

impl_from! { for NewMessageError ;
    ValidationError => |e| NewMessageError::Validation(e),
}

impl Message for NewMessage {
    type Result = Result<i32, NewMessageError>;
}

impl Handler<NewMessage> for Broker {
    type Result = Result<i32, NewMessageError>;

    fn handle(&mut self, msg: NewMessage, _: &mut Self::Context) -> Self::Result {
        let NewMessage { conversation, user, message } = msg;

        let ctx = format::validate(&message)?;
        let id = 0; // TODO: write message to database

        let event = Event {
            conversation, id, user,
            timestamp: Utc::now().naive_utc(),
            message,
        };

        for listener in self.listeners.get(&conversation).into_iter().flatten() {
            if let Err(err) = listener.addr.do_send(event.clone()) {
                error!("Can't send message to user {} in conversation {}: {}",
                    listener.user, conversation, err);
                ctx.notify(Disconnect {
                    conversation,
                    addr: listener.addr.clone(),
                });
            }

            // TODO: notify users who aren't currently connected to conversation
        }

        Ok(id)
    }
}

/// Notification about an event in a conversation.
#[derive(Clone)]
pub struct Event {
    /// Conversation in which this event occurred.
    pub conversation: i32,
    /// Message's ID.
    pub id: i32,
    /// User who send this message.
    pub user: i32,
    /// Time when this message was created.
    pub timestamp: NaiveDateTime,
    /// Message data.
    pub message: Bytes,
}

impl Message for Event {
    type Result = ();
}
