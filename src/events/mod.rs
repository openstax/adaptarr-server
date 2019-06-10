//! Handling of events and notifications.

use diesel::result::Error as DbError;
use failure::Fail;

mod events;
mod service;

pub use self::{
    events::*,
    service::{
        EventManager,
        NewEvent,
        Notify,
        RegisterListener,
        UnregisterListener,
    },
};

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    #[fail(display = "Error obtaining database connection: {}", _0)]
    DatabasePool(#[cause] r2d2::Error),
    #[fail(display = "Error serializing event data: {}", _0)]
    Serialize(#[cause] rmps::encode::Error),
    #[fail(display = "Error deserializing event data: {}", _0)]
    Deserialize(#[cause] rmps::decode::Error),
}

impl_from! { for Error ;
    DbError => |e| Error::Database(e),
    r2d2::Error => |e| Error::DatabasePool(e),
    rmps::encode::Error => |e| Error::Serialize(e),
    rmps::decode::Error => |e| Error::Deserialize(e),
}
