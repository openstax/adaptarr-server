use adaptarr_macros::From;
use diesel::{Connection as _, prelude::*, result::Error as DbError};
use failure::Fail;
use serde::Serialize;

use crate::{
    ApiError,
    db::{
        Connection,
        models as db,
        schema::{conversations, conversation_members},
    },
    models::User,
};

pub struct Conversation {
    data: db::Conversation,
}

/// A subset of conversation's data that can safely be publicly exposed.
#[derive(Serialize)]
pub struct PublicData {
    pub id: i32,
    pub members: Vec<i32>,
}

impl Conversation {
    /// Construct `Conversation` from its database counterpart.
    pub(super) fn from_db(data: db::Conversation) -> Self {
        Self { data }
    }

    /// Get list of all conversation a user has access to.
    pub fn all_of(db: &Connection, user: i32)
    -> Result<Vec<Conversation>, DbError> {
        conversation_members::table
            .filter(conversation_members::user.eq(user))
            .inner_join(conversations::table)
            .get_results::<(db::ConversationMember, db::Conversation)>(db)
            .map(|v| v.into_iter().map(|(_, c)| Conversation::from_db(c)).collect())
    }

    /// Find a conversation by ID.
    pub fn by_id(db: &Connection, id: i32)
    -> Result<Conversation, FindConversationError> {
        conversations::table
            .filter(conversations::id.eq(id))
            .get_result(db)
            .optional()?
            .ok_or(FindConversationError::NotFound)
            .map(Conversation::from_db)
    }

    /// Create a new conversation between users.
    pub fn create(db: &Connection, members: Vec<User>) -> Result<Self, DbError> {
        db.transaction(|| {
            let conversation = diesel::insert_into(conversations::table)
                .default_values()
                .get_result::<db::Conversation>(db)?;

            diesel::insert_into(conversation_members::table)
                .values(members.iter().map(|user| db::ConversationMember {
                    conversation: conversation.id,
                    user: user.id
                }).collect::<Vec<_>>())
                .execute(db)?;

            Ok(Conversation::from_db(conversation))
        })
    }

    /// Get the public portion of this conversation's data.
    pub fn get_public(&self, db: &Connection) -> Result<PublicData, DbError> {
        let db::Conversation { id, .. } = self.data;

        let members = conversation_members::table
            .filter(conversation_members::conversation.eq(id))
            .get_results::<db::ConversationMember>(db)?
            .into_iter()
            .map(|member| member.user)
            .collect();

        Ok(PublicData { id, members })
    }

    /// Check whether a user can access a conversation.
    pub fn check_access(&self, db: &Connection, user: i32)
    -> Result<bool, DbError> {
        let q = conversation_members::table
            .filter(conversation_members::conversation.eq(self.data.id)
                .and(conversation_members::user.eq(user)));
        diesel::select(diesel::dsl::exists(q)).get_result(db)
    }
}

impl std::ops::Deref for Conversation {
    type Target = db::Conversation;

    fn deref(&self) -> &db::Conversation {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum FindConversationError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// No conversation found matching given criteria.
    #[fail(display = "No such conversation")]
    #[api(code = "conversation:not-found", status = "NOT_FOUND")]
    NotFound,
}
