//! Actix actor handling creation and delivery of events.

use actix::{Actor, Addr, Context, Handler, Message, Recipient};
use chrono::NaiveDateTime;
use diesel::{
    prelude::*,
    result::Error as DbError,
};
use failure::Fail;
use serde::Serialize;
use std::collections::HashMap;

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
    type Result = ();
}

/// Message sent to all interested listeners when a new event is created.
///
/// To register for receiving this message send [`RegisterListener`]
/// to [`EventManager`].
pub struct NewEvent {
    pub id: i32,
    pub timestamp: NaiveDateTime,
    pub event: Event,
}

impl Message for NewEvent {
    type Result = ();
}

/// Register a new event listener for a given user.
pub struct RegisterListener {
    pub user: i32,
    pub addr: Recipient<NewEvent>,
}

impl Message for RegisterListener {
    type Result = ();
}

/// Unregister an event listener for a given user.
pub struct UnregisterListener {
    pub user: i32,
    pub addr: Recipient<NewEvent>,
}

impl Message for UnregisterListener {
    type Result = ();
}

/// Actix actor which manages persisting events and notifying users of them.
pub struct EventManager {
    pool: Pool,
    streams: HashMap<i32, Recipient<NewEvent>>,
}

impl EventManager {
    pub fn new(pool: Pool) -> EventManager {
        EventManager {
            pool,
            streams: HashMap::new(),
        }
    }

    /// Emit an event.
    ///
    /// This method will create a new database entry and notify event listeners.
    /// It will not however send out email notifications, as this is done
    /// periodically, not immediately after an event is created.
    fn notify(&mut self, msg: Notify) -> Result<(), Error> {
        let Notify { user, event } = msg;

        let db = self.pool.get()?;

        let mut data = Vec::new();
        event.serialize(&mut rmps::Serializer::new(&mut data))?;

        let ev = diesel::insert_into(events::table)
            .values(&db::NewEvent {
                user: user.id,
                kind: event.kind(),
                data: &data,
            })
            .get_result::<db::Event>(&*db)?;

        if let Some(stream) = self.streams.get(&user.id) {
            let _ = stream.do_send(NewEvent {
                id: ev.id,
                timestamp: ev.timestamp,
                event,
            });
        }

        Ok(())
    }
}

impl Actor for EventManager {
    type Context = Context<Self>;
}

impl Handler<Notify> for EventManager {
    type Result = ();

    fn handle(&mut self, msg: Notify, _: &mut Context<Self>) {
        match self.notify(msg) {
            Ok(()) => (),
            Err(err) => {
                eprint!("error sending notification: {}", err);
            }
        }
    }
}

impl Handler<RegisterListener> for EventManager {
    type Result = ();

    fn handle(&mut self, msg: RegisterListener, _: &mut Self::Context) {
        let RegisterListener { user, addr } = msg;
        self.streams.insert(user, addr);
    }
}

impl Handler<UnregisterListener> for EventManager {
    type Result = ();

    fn handle(&mut self, msg: UnregisterListener, _: &mut Self::Context) {
        let UnregisterListener { user, .. } = msg;
        self.streams.remove(&user);
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

pub trait EventManagerAddrExt {
    fn notify<E>(&self, user: User, event: E)
    where
        Event: From<E>;
}

impl EventManagerAddrExt for Addr<EventManager> {
    /// Emit an event.
    fn notify<E>(&self, user: User, event: E)
    where
        Event: From<E>,
    {
        self.do_send(Notify {
            user,
            event: Event::from(event),
        })
    }
}
