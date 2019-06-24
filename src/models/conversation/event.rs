use diesel::{prelude::*, result::Error as DbError};

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
