//! Handling of events and notifications.

use actix::{Addr, Arbiter};

use crate::db;

mod events;
mod service;

pub use self::{
    events::*,
    service::{EventManager, Notify},
};

/// Start an event manager instance.
pub fn start(pool: db::Pool) -> Addr<EventManager> {
    Arbiter::start(move |_| EventManager::new(pool.clone()))
}
