use chrono::{DateTime, Utc};
use diesel::{Connection as _, prelude::*, result::Error as DbError};
use serde::Serialize;

use crate::{
    audit,
    db::{Connection, models as db, schema::{users, support_tickets, support_ticket_authors}},
    events::{EventManager, NewSupportTicket},
};
use super::{AssertExists, FindModelResult, Model, User, conversation::Conversation};

pub struct Ticket {
    data: db::SupportTicket,
}

#[derive(Serialize)]
pub struct Public {
    id: i32,
    title: String,
    opened: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authors: Option<Vec<<User as Model>::Id>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation: Option<<Conversation as Model>::Public>,
}

impl Model for Ticket {
    const ERROR_CATEGORY: &'static str = "support:ticket";

    type Id = i32;
    type Database = db::SupportTicket;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, id: Self::Id) -> FindModelResult<Self> {
        support_tickets::table
            .filter(support_tickets::id.eq(id))
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

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Self::Public {
        let db::SupportTicket { id, ref title, opened, .. } = self.data;

        Public {
            id,
            title: title.clone(),
            opened,
            authors: None,
            conversation: None,
        }
    }

    fn get_public_full(&self, db: &Connection, _: &())
    -> Result<Self::Public, DbError> {
        let db::SupportTicket { id, ref title, opened, .. } = self.data;
        let conversation = self.conversation(db)?;

        Ok(Public {
            id,
            title: title.clone(),
            opened,
            authors: Some(self.authors(db)?),
            conversation: Some(conversation.get_public_full(db, &())?),
        })
    }
}

impl Ticket {
    /// Find by ID a ticket user has access to.
    pub fn by_id_and_user(db: &Connection, id: i32, user: &User)
    -> FindModelResult<Self> {
        support_tickets::table
            .inner_join(support_ticket_authors::table)
            .filter(support_tickets::id.eq(id)
                .and(support_ticket_authors::user.eq(user.id)))
            .select(support_tickets::all_columns)
            .get_result(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    /// Get all tickets.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all(db: &Connection) -> Result<Vec<Ticket>, DbError> {
        support_tickets::table
            .get_results(db)
            .map(Model::from_db)
    }

    /// Get all tickets opened by a user.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all_of(db: &Connection, user: &User) -> Result<Vec<Ticket>, DbError> {
        support_tickets::table
            .inner_join(support_ticket_authors::table)
            .filter(support_ticket_authors::user.eq(user.id))
            .select(support_tickets::all_columns)
            .get_results(db)
            .map(Model::from_db)
    }

    /// Create a new support ticket.
    pub fn create(db: &Connection, title: &str, author: &User)
    -> Result<Ticket, DbError> {
        db.transaction(|| {
            let conversation = Conversation::create(db, &[author.id])?;

            let data = diesel::insert_into(support_tickets::table)
                .values(db::NewSupportTicket {
                    title,
                    conversation: conversation.id,
                })
                .get_result::<db::SupportTicket>(db)?;

            diesel::insert_into(support_ticket_authors::table)
                .values(db::SupportTicketAuthor {
                    ticket: data.id,
                    user: author.id,
                })
                .execute(db)?;

            audit::log_db(db, "support_tickets", data.id, "create", LogCreation {
                title,
                conversation: data.id,
            });

            let support = users::table
                .filter(users::is_support.eq(true))
                .select(users::id)
                .get_results::<i32>(db)?;

            EventManager::notify(support, NewSupportTicket {
                author: author.id,
                ticket: data.id,
            });

            Ok(Ticket::from_db(data))
        })
    }

    pub fn conversation(&self, db: &Connection) -> Result<Conversation, DbError> {
        Conversation::by_id(db, self.data.conversation).assert_exists()
    }

    /// Get list of ticket's authors.
    pub fn authors(&self, db: &Connection)
    -> Result<Vec<<User as Model>::Id>, DbError> {
        support_ticket_authors::table
            .filter(support_ticket_authors::ticket.eq(self.data.id))
            .select(support_ticket_authors::user)
            .get_results(db)
    }

    /// Set ticket's title.
    pub fn set_title(&mut self, db: &Connection, title: &str)
    -> Result<(), DbError> {
        db.transaction(|| {
            audit::log_db(db, "support_tickets", self.data.id, "set-title", title);

            self.data = diesel::update(&self.data)
                .set(support_tickets::title.eq(title))
                .get_result(db)?;

            Ok(())
        })
    }
}

impl std::ops::Deref for Ticket {
    type Target = db::SupportTicket;

    fn deref(&self) -> &db::SupportTicket {
        &self.data
    }
}

#[derive(Serialize)]
struct LogCreation<'a> {
    title: &'a str,
    conversation: i32,
}
