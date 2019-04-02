//! Actix actor handling creation and delivery of events.

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, Recipient};
use chrono::{NaiveDateTime, Utc};
use diesel::{
    Connection as _,
    prelude::*,
    result::Error as DbError,
};
use serde::Serialize;
use std::{collections::HashMap, time::Duration};

use crate::{
    config::Config,
    db::{
        Connection,
        Pool,
        models as db,
        schema::events,
    },
    i18n::I18n,
    mail::Mailer,
    models::{
        module::{Module, FindModuleError},
        user::{User, FindUserError},
    },
    templates,
    utils::IteratorGroupExt,
};
use super::events::{Event, ExpandedEvent, ExpandedUser, ExpandedModule};

/// Interval between two notification emails.
///
/// It's set to 30 minutes in production and one minute in development.
#[cfg(any(not(debug_assertions), doc))]
const NOTIFY_INTERVAL: Duration = Duration::from_secs(1800);

#[cfg(all(debug_assertions, not(doc)))]
const NOTIFY_INTERVAL: Duration = Duration::from_secs(60);

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
    config: Config,
    pool: Pool,
    i18n: I18n<'static>,
    mail: Mailer,
    streams: HashMap<i32, Recipient<NewEvent>>,
    last_notify: NaiveDateTime,
}

impl EventManager {
    pub fn new(config: Config, pool: Pool, i18n: I18n<'static>, mail: Mailer)
    -> EventManager {
        EventManager {
            config,
            pool,
            i18n,
            mail,
            streams: HashMap::new(),
            last_notify: Utc::now().naive_utc(),
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

    fn on_interval(&mut self, _: &mut Context<Self>) {
        match self.send_emails() {
            Ok(()) => {}
            Err(err) => error!("Error sending email notifications: {}", err),
        }
    }

    /// Send email notifications for unread events.
    fn send_emails(&mut self) -> Result<(), Error> {
        let now = Utc::now().naive_utc();
        let db = self.pool.get()?;
        let dbcon = &*db;

        dbcon.transaction::<_, Error, _>(|| {
            let events = events::table
                .filter(events::timestamp.ge(self.last_notify)
                    .and(events::is_unread.eq(true)))
                .order((events::user, events::timestamp.asc()))
                .get_results::<db::Event>(&*db)?
                .into_iter()
                .group_by_key::<Vec<_>, _, _>(|event| event.user);

            for events in events {
                let user = match User::by_id(dbcon, events[0].user) {
                    Ok(user) => user,
                    Err(FindUserError::Internal(err)) => return Err(err.into()),
                    Err(FindUserError::NotFound) => panic!(
                        "Inconsistent database: user doesn't exist but owns \
                        an event",
                    ),
                };
                self.notify_user_by_email(&user, dbcon, events)?;
            }

            Ok(())
        })?;

        self.last_notify = now;

        Ok(())
    }

    /// Send email notifications to a particular user.
    fn notify_user_by_email(
        &mut self,
        user: &User,
        dbcon: &Connection,
        events: Vec<db::Event>,
    ) -> Result<(), Error> {
        let events = events.into_iter()
            .map(|event| -> Result<ExpandedEvent, Error> {
                let data = rmps::from_slice(&event.data)?;

                Ok(match data {
                    Event::Assigned(ev) => {
                        let who = match User::by_id(dbcon, ev.who) {
                            Ok(user) => user,
                            Err(FindUserError::Internal(err)) =>
                                return Err(err.into()),
                            Err(FindUserError::NotFound) => panic!(
                                "Inconsistent database: user doesn't exist \
                                but is referenced by an event",
                            ),
                        }.into_db();
                        let module = match Module::by_id(dbcon, ev.module) {
                            Ok(module) => module,
                            Err(FindModuleError::Database(err)) =>
                                return Err(err.into()),
                            Err(FindModuleError::NotFound) => panic!(
                                "Inconsistent database: module doesn't exist \
                                but is referenced by an event",
                            ),
                        }.into_db();

                        ExpandedEvent::Assigned {
                            who: ExpandedUser {
                                name: who.0.name,
                                url: format!("https://{}/users/{}",
                                    self.config.server.domain, who.0.id),
                            },
                            module: ExpandedModule {
                                title: module.1.title,
                                url: format!("https://{}/modules/{}",
                                    self.config.server.domain, module.0.id),
                            },
                        }
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let locale = self.i18n.find_locale(&user.language())
            .expect("user's preferred language to exist");

        let args = std::iter::once(
            ("count", fluent::FluentValue::from(events.len() as isize))
        ).collect();

        self.mail.send(
            "notify",
            user.mailbox(),
            ("mail-notify-subject", &args),
            &templates::NotifyMailArgs { events: &events },
            locale,
        );

        Ok(())
    }
}

impl Actor for EventManager {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(NOTIFY_INTERVAL, Self::on_interval);
    }
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
        let UnregisterListener { user, addr: _ } = msg;
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
    #[fail(display = "Error deserializing event data: {}", _0)]
    Deserialize(#[cause] rmps::decode::Error),
}

impl_from! { for Error ;
    DbError => |e| Error::Database(e),
    r2d2::Error => |e| Error::DatabasePool(e),
    rmps::encode::Error => |e| Error::Serialize(e),
    rmps::decode::Error => |e| Error::Deserialize(e),
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
