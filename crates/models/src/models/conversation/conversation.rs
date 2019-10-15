use diesel::{Connection as _, prelude::*, result::Error as DbError};
use serde::Serialize;

use crate::{
    db::{Connection, models as db, schema::{conversations, conversation_members}},
    models::{FindModelResult, Model, User},
};

pub struct Conversation {
    data: db::Conversation,
}

/// A subset of conversation's data that can safely be publicly exposed.
#[derive(Serialize)]
pub struct Public {
    pub id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<i32>>,
}

impl Model for Conversation {
    const ERROR_CATEGORY: &'static str = "conversation";

    type Id = i32;
    type Database = db::Conversation;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, id: i32) -> FindModelResult<Conversation> {
        conversations::table
            .filter(conversations::id.eq(id))
            .get_result(db)
            .map(Conversation::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Self { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Self::Public {
        let db::Conversation { id, .. } = self.data;

        Public {
            id,
            members: None,
        }
    }

    fn get_public_full(&self, db: &Connection, _: &())
    -> Result<Self::Public, DbError> {
        let db::Conversation { id, .. } = self.data;

        Ok(Public {
            id,
            members: Some(self.get_members(db)?),
        })
    }
}

impl Conversation {
    /// Get list of all conversation a user has access to.
    pub fn all_of(db: &Connection, user: i32)
    -> Result<Vec<Conversation>, DbError> {
        conversation_members::table
            .filter(conversation_members::user.eq(user))
            .inner_join(conversations::table)
            .get_results::<(db::ConversationMember, db::Conversation)>(db)
            .map(|v| v.into_iter().map(|(_, c)| Conversation::from_db(c)).collect())
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

    /// Get list of IDs of users who are members of this conversation.
    pub fn get_members(&self, db: &Connection) -> Result<Vec<i32>, DbError> {
        conversation_members::table
            .filter(conversation_members::conversation.eq(self.id))
            .order_by(conversation_members::user.asc())
            .select(conversation_members::user)
            .get_results(db)
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
