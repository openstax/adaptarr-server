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
use adaptarr_i18n::I18n;
use adaptarr_mail::Mailer;
use chrono::{NaiveDateTime, Utc};
use diesel::{Connection as _, prelude::*};
use itertools::Itertools;
use log::error;
use serde::Serialize;
use std::{borrow::Cow, collections::HashMap, sync::Arc, time::Duration};

use crate::{
    AssertExists,
    Config,
    Model,
    User,
    db::{Connection, Pool, models as db, schema::events},
};
use super::{
    Error,
    events::{Event, ExpandedEvent, Group, Kind, expand_event},
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
pub struct Notify<T: NotifyTarget> {
    pub target: T,
    pub event: Event,
}

impl<T: NotifyTarget> Message for Notify<T> {
    type Result = ();
}

/// Trait for types describing a target of a notification.
///
/// This abstraction is used to allow sending events to a variety of targets,
/// such as a raw user ID (`i32`), a user model (`User`), or a number of targets
/// at once (`Vec`).
pub trait NotifyTarget {
    type Iter: IntoIterator<Item = i32>;

    /// Convert this target into ID of users to notify.
    fn into_user_ids(self) -> Self::Iter;
}

pub trait IntoNotifyTarget {
    type Target: NotifyTarget;

    fn into_notify_target(self) -> Self::Target;
}

/// Message sent to all interested listeners when a new event is created.
///
/// To register for receiving this message send [`RegisterListener`]
/// to [`EventManager`].
pub struct NewEvent {
    pub id: i32,
    pub timestamp: NaiveDateTime,
    pub event: Arc<Event>,
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
    i18n: I18n<'static>,
    streams: HashMap<i32, Recipient<NewEvent>>,
    last_notify: NaiveDateTime,
}

impl EventManager {
    /// Emit an event.
    ///
    /// Errors will be logged, but otherwise ignored.
    pub fn notify<T, E>(target: T, event: E)
    where
        T: IntoNotifyTarget,
        T::Target: Send + 'static,
        Event: From<E>,
    {
        let manager = EventManager::from_registry();
        let message = Notify {
            target: target.into_notify_target(),
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
    fn do_notify<T: NotifyTarget>(&mut self, msg: Notify<T>) -> Result<(), Error> {
        let Notify { target, event } = msg;

        let db = self.pool.get()?;

        let mut data = Vec::new();
        event.serialize(&mut rmps::Serializer::new(&mut data))?;

        let event = Arc::new(event);

        for user in target.into_user_ids() {
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
                    event: event.clone(),
                });
            }
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
                let user = User::by_id(dbcon, user)
                    .assert_exists()?;
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
        let domain = Config::domain();

        let groups = events
            .into_iter()
            .group_by(|event| Kind::from_str(&event.kind).group());

        let mut groupped = Vec::new();

        for (kind, group) in groups.into_iter() {
            let evs = group
                .map(|event| expand_event(domain, dbcon, &event))
                .collect::<Result<Vec<_>, _>>()?;

            groupped.push((kind, evs));
        }

        let locale = self.i18n.find_locale(&user.language())
            .expect("user's preferred language to exist");

        Mailer::do_send(
            user.mailbox(),
            "notify",
            "mail-notify-subject",
            &NotifyMailArgs {
                events: &groupped,
                urls: NotifyMailArgsUrls {
                    notification_centre: format!("https://{}/notifications",
                        domain).into(),
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
            pool: crate::db::pool(),
            i18n: adaptarr_i18n::load()
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

impl<T: NotifyTarget> Handler<Notify<T>> for EventManager {
    type Result = ();

    fn handle(&mut self, msg: Notify<T>, _: &mut Context<Self>) {
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

impl<T: NotifyTarget> IntoNotifyTarget for T {
    type Target = T;

    fn into_notify_target(self) -> Self::Target {
        self
    }
}

impl NotifyTarget for User {
    type Iter = std::iter::Once<i32>;

    fn into_user_ids(self) -> Self::Iter {
        std::iter::once(self.id)
    }
}

impl IntoNotifyTarget for &User {
    type Target = i32;

    fn into_notify_target(self) -> Self::Target {
        self.id
    }
}

impl NotifyTarget for db::User {
    type Iter = std::iter::Once<i32>;

    fn into_user_ids(self) -> Self::Iter {
        std::iter::once(self.id)
    }
}

impl IntoNotifyTarget for &db::User {
    type Target = i32;

    fn into_notify_target(self) -> Self::Target {
        self.id
    }
}

impl NotifyTarget for i32 {
    type Iter = std::iter::Once<i32>;

    fn into_user_ids(self) -> Self::Iter {
        std::iter::once(self)
    }
}

impl<T: NotifyTarget> NotifyTarget for Vec<T> {
    #[allow(clippy::type_complexity)]
    type Iter = std::iter::FlatMap<
        std::vec::IntoIter<T>,
        <T as NotifyTarget>::Iter,
        fn(T) -> <T as NotifyTarget>::Iter,
    >;

    fn into_user_ids(self) -> Self::Iter {
        self.into_iter().flat_map(NotifyTarget::into_user_ids)
    }
}

/// Arguments for `mail/notify`.
#[derive(Serialize)]
struct NotifyMailArgs<'a> {
    /// List of new events to include in the email.
    events: &'a [(Group, Vec<ExpandedEvent>)],
    // /// Various URLs which can be used in the email.
    urls: NotifyMailArgsUrls<'a>,
}

#[derive(Serialize)]
struct NotifyMailArgsUrls<'a> {
    notification_centre: Cow<'a, str>,
}
