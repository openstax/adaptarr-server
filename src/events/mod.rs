//! Handling of events and notifications.

use adaptarr_macros::From;
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

#[derive(Debug, Fail, From)]
pub enum Error {
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] #[from] DbError),
    #[fail(display = "Error obtaining database connection: {}", _0)]
    DatabasePool(#[cause] #[from] r2d2::Error),
    #[fail(display = "Error serializing event data: {}", _0)]
    Serialize(#[cause] #[from] rmps::encode::Error),
    #[fail(display = "Unknown event type: {:?}", _0)]
    UnknownEvent(String),
    #[fail(display = "Error deserializing event data: {}", _0)]
    Deserialize(#[cause] #[from] rmps::decode::Error),
}
