use diesel::{prelude::*, result::Error as DbError};

use crate::{
    db::{Connection, models as db, schema::conversation_events},
    models::{FindModelResult, Model},
};
use super::format::Validation;

#[derive(Debug)]
pub struct Event {
    data: db::ConversationEvent,
}

impl Model for Event {
    const ERROR_CATEGORY: &'static str = "conversation-event";

    type Id = i32;
    type Database = db::ConversationEvent;
    type Public = ();
    type PublicParams = ();

    fn by_id(db: &Connection, id: i32) -> FindModelResult<Self> {
        conversation_events::table
            .filter(conversation_events::id.eq(id))
            .get_result(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Self { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> i32 {
        self.data.id
    }

    fn get_public(&self) -> Self::Public {}
}

impl Event {
    /// Create a new message in a conversation.
    pub fn new_message_in(
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
                data: message.body.as_ref(),
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
