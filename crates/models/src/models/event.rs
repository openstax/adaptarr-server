use diesel::{prelude::*, result::Error as DbError};

use crate::db::{Connection, models as db, schema::events};
use super::{FindModelResult, Model};

pub use crate::events::Event as Public;

#[derive(Debug)]
pub struct Event {
    data: db::Event,
}

impl Model for Event {
    const ERROR_CATEGORY: &'static str = "event";

    type Id = (i32, i32);
    type Database = db::Event;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, (id, user): Self::Id) -> FindModelResult<Self> {
        events::table
            .filter(events::user.eq(user)
                .and(events::id.eq(id)))
            .get_result::<db::Event>(db)
            .map_err(From::from)
            .map(Self::from_db)
    }

    fn from_db(data: Self::Database) -> Self {
        Event { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        (self.data.id, self.data.user)
    }

    fn get_public(&self) -> Self::Public {
        Public::load(&self.data.kind, &self.data.data)
            .expect("can't unpack event data")
    }
}

impl Event {
    /// Get all unread events.
    pub fn unread(db: &Connection, user: i32) -> Result<Vec<Event>, DbError> {
        events::table
            .filter(events::user.eq(user)
                .and(events::is_unread.eq(true)))
            .get_results::<db::Event>(db)
            .map(|v| v.into_iter().map(|data| Event { data }).collect())
    }

    /// Change this event's unread state.
    pub fn set_unread(&mut self, db: &Connection, is_unread: bool)
    -> Result<(), DbError> {
        diesel::update(&self.data)
            .set(events::is_unread.eq(is_unread))
            .execute(db)?;
        self.data.is_unread = is_unread;
        Ok(())
    }
}

impl std::ops::Deref for Event {
    type Target = db::Event;

    fn deref(&self) -> &db::Event {
        &self.data
    }
}
