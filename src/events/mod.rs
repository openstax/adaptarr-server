//! Handling of events and notifications.

use actix::{Addr, Arbiter};
use diesel::result::Error as DbError;

use crate::{config::Config, db, i18n::I18n, mail::Mailer};

mod events;
mod service;

pub use self::{
    events::*,
    service::{
        EventManager,
        EventManagerAddrExt,
        NewEvent,
        Notify,
        RegisterListener,
        UnregisterListener,
    },
};

/// Start an event manager instance.
pub fn start(cfg: Config, pool: db::Pool, i18n: I18n<'static>, mail: Mailer)
-> Addr<EventManager> {
    Arbiter::start(move |_| EventManager::new(
        cfg.clone(), pool.clone(), i18n, mail.clone()))
}

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
