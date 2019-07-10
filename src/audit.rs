use diesel::prelude::*;
use futures::{task_local, task::is_in_task};
use serde::Serialize;
use std::cell::Cell;
use uuid::Uuid;

use crate::db::{
    Connection,
    pool,
    models as db,
    schema::audit_log,
};

std::thread_local! {
    static THREAD_ACTOR: Cell<Option<Actor>> = Cell::new(None);
}

task_local! {
    static ACTOR: Cell<Option<Actor>> = Cell::new(None)
}

/// Entity responsible for an action.
#[derive(Clone, Copy, Debug)]
pub enum Actor {
    /// System. This actor is used for actions carried automatically by the
    /// system, and actions invoked from the CLI.
    System,
    /// A user.
    User(i32),
}

impl Actor {
    fn as_db(&self) -> Option<i32> {
        match *self {
            Actor::System => None,
            Actor::User(id) => Some(id),
        }
    }
}

impl From<i32> for Actor {
    fn from(id: i32) -> Self {
        Actor::User(id)
    }
}

/// Set actor associated with current task/thread, returning previous one, if
/// any.
pub fn set_actor<A>(actor: A) -> Option<Actor>
where
    Option<Actor>: From<A>,
{
    let actor = Option::from(actor);
    if is_in_task() {
        ACTOR.with(|c| c.replace(actor))
    } else {
        THREAD_ACTOR.with(|c| c.replace(actor))
    }
}

/// Get actor associated with current task/thread.
///
/// ## Panics
///
/// This function will panic if current task/thread has no actor associated with
/// it (see [`set_actor()`]).
pub fn get_actor() -> Actor {
    if is_in_task() {
        ACTOR.with(Cell::get)
    } else {
        THREAD_ACTOR.with(Cell::get)
    }.expect("no audit actor registered on current task")
}

/// Run closure in such context that all actions it causes are attributed to the
/// specified actor.
pub fn with_actor<A, F, R>(actor: A, f: F) -> R
where
    Option<Actor>: From<A>,
    F: FnOnce() -> R,
{
    let old = set_actor(actor);
    let r = f();
    // XXX: Not sure why, but without turbofish rustc complains that “expected
    // type parameter (A), found enum `std::option::Option`”.
    set_actor::<Option<Actor>>(old);
    r
}

/// Store an event in the audit log.
///
/// This method should not be used within an existing database transaction as it
/// will create log entries regardless of whether the transaction is committed
/// or aborted. Inside existing transactions use [`log_db()`] instead.
///
/// This function can only be called in context of a [`futures::task::Task`].
/// For a non-asynchronous version see [`log_actor()`].
///
/// ## Panics
///
/// This function will panic if current task/thread has no actor associated with
/// it (see [`set_actor()`]).
pub fn log<I, D>(context: &str, context_id: I, kind: &str, data: D)
where
    ContextId: From<I>,
    D: Serialize,
{
    let db = pool()
        .and_then(|db| db.get().map_err(From::from))
        .expect("database connection should be established before storing \
            audit log entries");
    log_db_actor(&*db, get_actor(), context, context_id, kind, data);
}

/// Store an event in the audit log.
///
/// This method should not be used within an existing database transaction as it
/// will create log entries regardless of whether the transaction is committed
/// or aborted. Inside existing transactions use [`log_db()`] instead.
pub fn log_actor<A, I, D>(
    actor: A,
    context: &str,
    context_id: I,
    kind: &str,
    data: D,
)
where
    Actor: From<A>,
    ContextId: From<I>,
    D: Serialize,
{
    let db = pool()
        .and_then(|db| db.get().map_err(From::from))
        .expect("database connection should be established before storing \
            audit log entries");
    log_db_actor(&*db, actor, context, context_id, kind, data);
}

/// Store an event in the audit log.
///
/// This is a version of [`log()`] which takes an explicit database connection,
/// and can safely be used inside an existing transaction, only adding the event
/// when the transaction is committed.
///
/// ## Panics
///
/// This function will panic if current task/thread has no actor associated with
/// it (see [`set_actor()`]).
pub fn log_db<I, D>(
    db: &Connection,
    context: &str,
    context_id: I,
    kind: &str,
    data: D,
)
where
    ContextId: From<I>,
    D: Serialize,
{
    log_db_actor(db, get_actor(), context, context_id, kind, data);
}

/// Store an event in the audit log.
///
/// This is a version of [`log()`] which takes an explicit database connection,
/// and can safely be used inside an existing transaction, only adding the event
/// when the transaction is committed.
pub fn log_db_actor<A, I, D>(
    db: &Connection,
    actor: A,
    context: &str,
    context_id: I,
    kind: &str,
    data: D,
)
where
    Actor: From<A>,
    ContextId: From<I>,
    D: Serialize,
{
    let actor = Actor::from(actor).as_db();
    let (context_id, context_uuid) = ContextId::from(context_id).into_db();

    let data = rmps::to_vec_named(&data).expect("invalid audit log data");

    diesel::insert_into(audit_log::table)
        .values(db::NewAuditLog {
            actor,
            context,
            context_id,
            context_uuid,
            kind,
            data: &data,
        })
        .execute(db)
        .expect("could not save audit log entry");
}

pub enum ContextId {
    Integer(i32),
    Uuid(Uuid),
}

impl ContextId {
    fn into_db(self) -> (Option<i32>, Option<Uuid>) {
        match self {
            ContextId::Integer(id) => (Some(id), None),
            ContextId::Uuid(id) => (None, Some(id)),
        }
    }
}

impl From<i32> for ContextId {
    fn from(id: i32) -> Self {
        ContextId::Integer(id)
    }
}

impl From<Uuid> for ContextId {
    fn from(id: Uuid) -> Self {
        ContextId::Uuid(id)
    }
}
