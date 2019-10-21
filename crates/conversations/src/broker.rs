use actix::prelude::*;
use adaptarr_macros::From;
use adaptarr_models::{
    FindModelError,
    Model,
    conversation::format::{self, Error as ValidationError},
    db::{Pool, models as db, schema::conversation_events},
    events::{EventManager, NewMessage as NewMessageEvent},
    models::conversation::{
        Conversation as ConversationModel,
        Event as EventModel,
    },
};
use bytes::Bytes;
use diesel::{prelude::*, result::Error as DbError};
use failure::Fail;
use log::error;
use std::collections::hash_map::{Entry, HashMap};

use super::protocol;

/// Broker messages and events to users.
pub struct Broker {
    /// Mapping from conversation ID to a list of listeners for that
    /// conversation.
    conversations: HashMap<i32, Conversation>,
    pool: Pool,
}

impl Default for Broker {
    fn default() -> Self {
        Self {
            conversations: HashMap::new(),
            pool: adaptarr_models::db::pool(),
        }
    }
}

struct Conversation {
    /// List of IDs of users who are members of this conversation.
    members: Vec<i32>,
    /// List of listeners currently observing this conversation.
    listeners: Listeners,
}

/// Wrapper around a `Vec` which keeps its elements sorted (like a `BTreeSet`,
/// but also implements `retain`, and is slower).
#[derive(Default)]
struct Listeners(Vec<Listener>);

impl Listeners {
    fn insert(&mut self, listener: Listener) {
        let inx = match self.0.binary_search_by_key(&listener.user, |l| l.user) {
            Ok(inx) => inx,
            Err(inx) => inx,
        };
        self.0.insert(inx, listener);
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&Listener) -> bool,
    {
        self.0.retain(f)
    }

    fn iter(&self) -> impl Iterator<Item = &Listener> {
        self.0.iter()
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

#[derive(Debug, Fail, From)]
pub enum ConnectError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] #[from] DbError),
    /// Database pool error.
    #[fail(display = "database pool error: {}", _0)]
    Pool(#[cause] #[from] r2d2::Error),
    /// No conversation found matching given criteria.
    #[fail(display = "No such conversation")]
    NotFound,
}

impl From<FindModelError<ConversationModel>> for ConnectError {
    fn from(e: FindModelError<ConversationModel>) -> Self {
        match e {
            FindModelError::Database(_, e) => ConnectError::Database(e),
            FindModelError::NotFound(_) => ConnectError::NotFound,
        }
    }
}

impl Message for Connect {
    type Result = Result<(), ConnectError>;
}

impl Handler<Connect> for Broker {
    type Result = Result<(), ConnectError>;

    fn handle(&mut self, msg: Connect, _: &mut Self::Context) -> Self::Result {
        let Connect { user, conversation, addr } = msg;

        // TODO: verify the user can access this conversation

        // entry.try_insert_with
        let conversation = match self.conversations.entry(conversation) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let db = self.pool.get()?;
                let conversation = ConversationModel::by_id(&*db, conversation)?;
                let members = conversation.get_members(&*db)?;

                entry.insert(Conversation {
                    members,
                    listeners: Listeners::default(),
                })
            }
        };

        conversation.listeners.insert(Listener { user, addr });

        Ok(())
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

        if let Entry::Occupied(mut entry) = self.conversations.entry(conversation) {
            entry.get_mut().listeners.retain(|l| l.addr != addr);

            if entry.get().listeners.is_empty() {
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
    #[fail(display = "client is not connected to requested conversation")]
    NotConnected,
}

impl Message for NewMessage {
    type Result = Result<i32, NewMessageError>;
}

impl Handler<NewMessage> for Broker {
    type Result = Result<i32, NewMessageError>;

    fn handle(&mut self, msg: NewMessage, ctx: &mut Self::Context) -> Self::Result {
        let NewMessage { conversation: conversation_id, user: author, message } = msg;

        let conversation = self.conversations.get(&conversation_id)
            .ok_or(NewMessageError::NotConnected)?;

        let validation = format::validate(&message)?;

        let db = self.pool.get()?;
        let event = EventModel::new_message_in(
            &*db, conversation_id, author, &validation)?;
        let db::ConversationEvent {
            id, timestamp, data, ..
        } = event.into_db();

        let event = Event::NewMessage(protocol::NewMessage {
            id, timestamp,
            user: author,
            message: Bytes::from(data),
        });

        let mut listeners = conversation.listeners.iter();
        let mut members = conversation.members.iter();

        let mut listener = listeners.next();
        let mut member = members.next();

        while let Some(lst) = listener {
            while let Some(&user) = member {
                if user >= lst.user {
                    break
                }

                EventManager::notify(user, NewMessageEvent {
                    author,
                    conversation: conversation_id,
                    message: id,
                });

                member = members.next();
            }

            let user = lst.user;

            while let Some(lst) = listener {
                if lst.user != user {
                    break;
                }

                if let Err(err) = lst.addr.do_send(event.clone()) {
                    error!("Can't send message to user {} in conversation {}: {}",
                        lst.user, conversation_id, err);
                    ctx.notify(Disconnect {
                        conversation: conversation_id,
                        addr: lst.addr.clone(),
                    });
                }

                listener = listeners.next();
            }

            member = members.next();
        }

        while let Some(user) = member {
            EventManager::notify(*user, NewMessageEvent {
                author,
                conversation: conversation_id,
                message: id,
            });

            member = members.next();
        }

        Ok(id)
    }
}

/// Notification about an event in a conversation.
#[derive(Clone)]
pub enum Event {
    NewMessage(protocol::NewMessage),
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
    /// Number of events before reference to retrieve.
    pub number_before: u16,
    /// Number of events after reference to retrieve.
    pub number_after: u16,
}

#[derive(Debug, Fail, From)]
pub enum GetHistoryError {
    #[fail(display = "internal error")]
    Database(#[cause] #[from] DbError),
    #[fail(display = "internal error")]
    DbPool(#[cause] #[from] r2d2::Error),
}

pub struct History {
    pub before: Vec<db::ConversationEvent>,
    pub after: Vec<db::ConversationEvent>,
}

impl Message for GetHistory {
    type Result = Result<History, GetHistoryError>;
}

impl Handler<GetHistory> for Broker {
    type Result = Result<History, GetHistoryError>;

    fn handle(&mut self, msg: GetHistory, _: &mut Self::Context) -> Self::Result {
        let db = self.pool.get()?;

        db.transaction(|| {
            let mut before;
            let after;

            match msg.from {
                Some(id) => {
                    let reference = conversation_events::table
                        .filter(conversation_events::conversation.eq(msg.conversation)
                            .and(conversation_events::id.eq(id)))
                        .get_result::<db::ConversationEvent>(&*db)?;

                    before = conversation_events::table
                        .filter(conversation_events::conversation.eq(msg.conversation)
                            .and(conversation_events::timestamp.lt(reference.timestamp)))
                        .order_by(conversation_events::timestamp.desc())
                        .limit(i64::from(msg.number_before.min(64)))
                        .get_results(&*db)?;

                    after = conversation_events::table
                        .filter(conversation_events::conversation.eq(msg.conversation)
                            .and(conversation_events::timestamp.ge(reference.timestamp)))
                        .order_by(conversation_events::timestamp.asc())
                        .limit(i64::from((msg.number_after + 1).min(64)))
                        .get_results(&*db)?;
                }
                None => {
                    before = conversation_events::table
                        .filter(conversation_events::conversation.eq(msg.conversation))
                        .order_by(conversation_events::timestamp.desc())
                        .limit(i64::from(msg.number_before.min(128)))
                        .get_results(&*db)?;

                    after = Vec::new();
                }
            }

            before.reverse();
            Ok(History { before, after })
        })
    }
}
