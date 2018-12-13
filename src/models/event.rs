use actix_web::{HttpResponse, ResponseError};
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

    /// Find an event belonging to an user by ID.
    pub fn by_id(dbconn: &Connection, id: i32, user: i32)
    -> Result<Event, FindEventError> {
        events::table
            .filter(events::user.eq(user)
                .and(events::id.eq(id)))
            .get_result::<db::Event>(dbconn)
            .map_err(Into::into)
            .map(|data| Event { data })
    }

    /// Load this event's data.
    pub fn load(&self) -> EventData {
        rmp_serde::from_slice(&self.data.data).expect("can't unpack event data")
    }

    /// Change this event's unread state.
    pub fn set_unread(&mut self, dbconn: &Connection, is_unread: bool)
    -> Result<(), DbError> {
        diesel::update(&self.data)
            .set(events::is_unread.eq(is_unread))
            .execute(dbconn)?;
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

#[derive(Debug, Fail)]
pub enum FindEventError {
    /// Database error
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// No event matching given criteria found
    #[fail(display = "Event not found")]
    NotFound,
}

impl_from! { for FindEventError ;
    DbError => |e| match e {
        DbError::NotFound => FindEventError::NotFound,
        e => FindEventError::Database(e),
    },
}

impl ResponseError for FindEventError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            FindEventError::Database(_) => HttpResponse::InternalServerError()
                .finish(),
            FindEventError::NotFound => HttpResponse::NotFound()
                .finish(),
        }
    }
}
