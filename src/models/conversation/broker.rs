use actix::prelude::*;
use adaptarr_macros::From;
use bytes::Bytes;
use chrono::NaiveDateTime;
use diesel::{prelude::*, result::Error as DbError};
use failure::Fail;
use std::collections::hash_map::{Entry, HashMap};

use crate::{
    db::{Pool, models as db, schema::conversation_events},
    models::conversation::{
        event::Event as EventModel,
        format::{self, Error as ValidationError},
    },
};

/// Broker messages and events to users.
pub struct Broker {
    /// Mapping from conversation ID to a list of listeners for that
    /// conversation.
    listeners: HashMap<i32, Vec<Listener>>,
    pool: Pool,
}

impl Default for Broker {
    fn default() -> Self {
        Self {
            listeners: HashMap::new(),
            pool: crate::db::pool().expect("database should be initialized"),
        }
    }
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

#[derive(Debug, Fail, From)]
pub enum NewMessageError {
    #[fail(display = "malformed message: {}", _0)]
    Validation(#[cause] #[from] ValidationError),
    #[fail(display = "internal error")]
    Database(#[cause] #[from] DbError),
    #[fail(display = "internal error")]
    DbPool(#[cause] #[from] r2d2::Error),
}

impl Message for NewMessage {
    type Result = Result<i32, NewMessageError>;
}

impl Handler<NewMessage> for Broker {
    type Result = Result<i32, NewMessageError>;

    fn handle(&mut self, msg: NewMessage, ctx: &mut Self::Context) -> Self::Result {
        let NewMessage { conversation, user, message } = msg;

        let validation = format::validate(&message)?;

        let db = self.pool.get()?;
        let event = EventModel::new_message_in(
            &*db, conversation, user, &validation)?;
        let db::ConversationEvent {
            id, timestamp, data, ..
        } = event.into_db();

        let event = Event {
            id, conversation, user, timestamp,
            message: Bytes::from(data),
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

        Ok(event.id)
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

/// Request for slice of a conversation's history.
pub struct GetHistory {
    /// Conversation from which to retrieve history.
    pub conversation: i32,
    /// ID of an event relative to which to search.
    pub from: Option<i32>,
    /// Number of events to retrieve.
    pub number: u16,
}

#[derive(Debug, Fail, From)]
pub enum GetHistoryError {
    #[fail(display = "internal error")]
    Database(#[cause] #[from] DbError),
    #[fail(display = "internal error")]
    DbPool(#[cause] #[from] r2d2::Error),
}

impl Message for GetHistory {
    type Result = Result<Vec<db::ConversationEvent>, GetHistoryError>;
}

impl Handler<GetHistory> for Broker {
    type Result = Result<Vec<db::ConversationEvent>, GetHistoryError>;

    fn handle(&mut self, msg: GetHistory, _: &mut Self::Context) -> Self::Result {
        let db = self.pool.get()?;

        match msg.from {
            Some(id) => db.transaction(|| {
                let reference = conversation_events::table
                    .filter(conversation_events::conversation.eq(msg.conversation)
                        .and(conversation_events::id.eq(id)))
                    .get_result::<db::ConversationEvent>(&*db)?;

                conversation_events::table
                    .filter(conversation_events::conversation.eq(msg.conversation)
                        .and(conversation_events::timestamp.le(reference.timestamp)))
                    .order_by(conversation_events::timestamp.desc())
                    .limit(msg.number.min(128) as i64)
                    .get_results(&*db)
            }),
            None => conversation_events::table
                .filter(conversation_events::conversation.eq(msg.conversation))
                .order_by(conversation_events::timestamp.desc())
                .limit(msg.number.min(128) as i64)
                .get_results(&*db),
        }.map_err(From::from)
    }
}
