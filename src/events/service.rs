//! Actix actor handling creation and delivery of events.

use actix::{Actor, Context, Handler, Message};
use diesel::{
    prelude::*,
    result::Error as DbError,
};
use serde::Serialize;

use crate::{
    db::{
        Pool,
        models as db,
        schema::events,
    },
    models::User,
};
use super::events::Event;

/// Notify a user of an event.
///
/// After receiving this message the event manager will persist `event` in
/// the database, and attempt to notify the user.
pub struct Notify {
    pub user: User,
    pub event: Event,
}

impl Message for Notify {
    type Result = Result<(), Error>;
}

/// Actix actor which manages persisting events and notifying users of them.
pub struct EventManager {
    pool: Pool,
}

impl EventManager {
    pub fn new(pool: Pool) -> EventManager {
        EventManager { pool }
    }
}

impl Actor for EventManager {
    type Context = Context<Self>;
}

impl Handler<Notify> for EventManager {
    type Result = Result<(), Error>;

    fn handle(&mut self, msg: Notify, _: &mut Context<Self>) -> Result<(), Error> {
        let Notify { user, event } = msg;

        let db = self.pool.get()?;

        let mut data = Vec::new();
        event.serialize(&mut rmps::Serializer::new(&mut data))?;

        diesel::insert_into(events::table)
            .values(&db::NewEvent {
                user: user.id,
                kind: event.kind(),
                data: &data,
            })
            .execute(&*db)?;

        Ok(())
    }
}

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    #[fail(display = "Error obtaining database connection: {}", _0)]
    DatabasePool(#[cause] r2d2::Error),
    #[fail(display = "Error serializing event data: {}", _0)]
    Serialize(#[cause] rmps::encode::Error),
}

impl_from! { for Error ;
    DbError => |e| Error::Database(e),
    r2d2::Error => |e| Error::DatabasePool(e),
    rmps::encode::Error => |e| Error::Serialize(e),
}
