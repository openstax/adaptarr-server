//! Actix actor handling creation and delivery of events.

use actix::{
    Actor,
    AsyncContext,
    Context,
    Handler,
    Message,
    Recipient,
    Supervised,
    SystemService,
};
use chrono::{NaiveDateTime, Utc};
use diesel::{
    Connection as _,
    prelude::*,
};
use itertools::Itertools;
use serde::Serialize;
use std::{collections::HashMap, time::Duration};

use crate::{
    config::{self, Config},
    db::{
        Connection,
        Pool,
        models as db,
        schema::events,
    },
    i18n::{self, I18n},
    mail::Mailer,
    models::user::{User, FindUserError},
    templates,
};
use super::{
    Error,
    events::{Event, Kind, expand_event},
};

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
    pub user: i32,
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
    config: &'static Config,
    pool: Pool,
    i18n: I18n<'static>,
    streams: HashMap<i32, Recipient<NewEvent>>,
    last_notify: NaiveDateTime,
}

impl EventManager {
    /// Emit an event.
    ///
    /// Errors will be logged, but otherwise ignored.
    pub fn notify<E>(user: &db::User, event: E)
    where
        Event: From<E>,
    {
        let manager = EventManager::from_registry();
        let message = Notify {
            user: user.id,
            event: Event::from(event),
        };

        if let Err(err) = manager.try_send(message) {
            error!("Could not dispatch event notification: {}", err);
        }
    }

    /// Emit an event.
    ///
    /// This method will create a new database entry and notify event listeners.
    /// It will not however send out email notifications, as this is done
    /// periodically, not immediately after an event is created.
    fn do_notify(&mut self, msg: Notify) -> Result<(), Error> {
        let Notify { user, event } = msg;

        let db = self.pool.get()?;

        let mut data = Vec::new();
        event.serialize(&mut rmps::Serializer::new(&mut data))?;

        let ev = diesel::insert_into(events::table)
            .values(&db::NewEvent {
                user,
                kind: event.kind(),
                data: &data,
            })
            .get_result::<db::Event>(&*db)?;

        if let Some(stream) = self.streams.get(&user) {
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
                .group_by(|event| event.user);

            for (user, events) in events.into_iter() {
                let user = match User::by_id(dbcon, user) {
                    Ok(user) => user,
                    Err(FindUserError::Internal(err)) => return Err(err.into()),
                    Err(FindUserError::NotFound) => panic!(
                        "Inconsistent database: user doesn't exist but owns \
                        an event",
                    ),
                };
                self.notify_user_by_email(&user, dbcon, events.collect())?;
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
        let groups = events
            .into_iter()
            .group_by(|event| Kind::from_str(&event.kind));

        let mut groupped = Vec::new();

        for (kind, group) in groups.into_iter() {
            let evs = group.into_iter()
                .map(|event| expand_event(&self.config, dbcon, &event))
                .collect::<Result<Vec<_>, _>>()?;

            groupped.push((kind, evs));
        }

        let locale = self.i18n.find_locale(&user.language())
            .expect("user's preferred language to exist");

        Mailer::send(
            user.mailbox(),
            "notify",
            "mail-notify-subject",
            &templates::NotifyMailArgs {
                events: &groupped,
                urls: templates::NotifyMailArgsUrls {
                    notification_centre: format!("https://{}/notifications",
                        self.config.server.domain).into(),
                },
            },
            locale,
        );

        Ok(())
    }
}

impl Default for EventManager {
    fn default() -> Self {
        Self {
            config: config::load().expect("Configuration is not loaded"),
            pool: crate::db::pool().expect("Database is not initialized"),
            i18n: i18n::load()
                .expect("Internationalization subsystem is not loaded")
                .clone(),
            streams: HashMap::new(),
            last_notify: Utc::now().naive_utc(),
        }
    }
}

impl Actor for EventManager {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(NOTIFY_INTERVAL, Self::on_interval);
    }
}

impl Supervised for EventManager {
}

impl SystemService for EventManager {
}

impl Handler<Notify> for EventManager {
    type Result = ();

    fn handle(&mut self, msg: Notify, _: &mut Context<Self>) {
        match self.do_notify(msg) {
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
