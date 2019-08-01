use adaptarr_macros::From;
use diesel::{prelude::*, result::Error as DbError};
use failure::Fail;

use crate::db::{Connection, models as db, schema::conversation_events};
use super::format::Validation;

#[derive(Debug)]
pub struct Event {
    data: db::ConversationEvent,
}

impl Event {
    /// Construct `Event` from its database counterpart.
    pub(super) fn from_db(data: db::ConversationEvent) -> Self {
        Self { data }
    }

    /// Find a conversation event by ID.
    pub fn by_id(db: &Connection, id: i32) -> Result<Self, FindEventError> {
        conversation_events::table
            .filter(conversation_events::id.eq(id))
            .get_result(db)
            .optional()?
            .map(Self::from_db)
            .ok_or(FindEventError::NotFound)
    }

    /// Get underlying database model.
    pub fn into_db(self) -> db::ConversationEvent {
        self.data
    }

    /// Create a new message in a conversation.
    pub(super) fn new_message_in(
        db: &Connection,
        conversation: i32,
        author: i32,
        message: &Validation,
    ) -> Result<Self, DbError> {
        diesel::insert_into(conversation_events::table)
            .values(db::NewConversationEvent {
                conversation,
                kind: "new-message",
                author: Some(author),
                data: message.body,
            })
            .get_result(db)
            .map(Event::from_db)
    }
}

impl std::ops::Deref for Event {
    type Target = db::ConversationEvent;

    fn deref(&self) -> &db::ConversationEvent {
        &self.data
    }
}

#[derive(Debug, Fail, From)]
pub enum FindEventError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] #[from] DbError),
    /// No event found matching given criteria.
    #[fail(display = "No such event")]
    NotFound,
}
