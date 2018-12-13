use diesel::{
    prelude::*,
    result::Error as DbError,
};

use crate::{
    db::{
        Connection,
        models as db,
        schema::events,
    },
    events::Event as EventData,
};

#[derive(Debug)]
pub struct Event {
    data: db::Event,
}

impl Event {
    /// Get all unread events.
    pub fn unread(dbconn: &Connection, user: i32) -> Result<Vec<Event>, DbError> {
        events::table
            .filter(events::user.eq(user)
                .and(events::is_unread.eq(true)))
            .get_results::<db::Event>(dbconn)
            .map(|v| v.into_iter().map(|data| Event { data }).collect())
    }

    /// Load this event's data.
    pub fn load(&self) -> EventData {
        rmp_serde::from_slice(&self.data.data).expect("can't unpack event data")
    }
}

impl std::ops::Deref for Event {
    type Target = db::Event;

    fn deref(&self) -> &db::Event {
        &self.data
    }
}
