//! Handling of events and notifications.

use actix::{Addr, Arbiter};

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
