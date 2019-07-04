use failure::Fail;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::Config,
    db::{
        Connection,
        models as db,
        types::SlotPermission,
    },
    models::{
        book::{Book, FindBookError},
        editing::{
            Step,
            slot::{Slot, FindSlotError},
        },
        module::{Module, FindModuleError},
        user::{User, FindUserError},
    },
};
use super::Error;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Event {
    Assigned(Assigned),
    ProcessEnded(ProcessEnded),
    SlotFilled(SlotFilled),
    SlotVacated(SlotVacated),
    DraftAdvanced(DraftAdvanced),
}

impl Event {
    pub fn load(kind: &str, data: &[u8]) -> Result<Event, LoadEventError> {
        match Kind::from_str(&kind) {
            Kind::Assigned =>
                Ok(Event::Assigned(rmps::from_slice(&data)?)),
            Kind::ProcessEnded =>
                Ok(Event::ProcessEnded(rmps::from_slice(&data)?)),
            Kind::SlotFilled =>
                Ok(Event::SlotFilled(rmps::from_slice(&data)?)),
            Kind::SlotVacated =>
                Ok(Event::SlotVacated(rmps::from_slice(&data)?)),
            Kind::DraftAdvanced =>
                Ok(Event::DraftAdvanced(rmps::from_slice(&data)?)),
            Kind::Other => Err(LoadEventError::UnknownEvent(kind.to_string())),
        }
    }
}

#[derive(Debug, Fail)]
pub enum LoadEventError {
    #[fail(display = "unknown event type: {}", _0)]
    UnknownEvent(String),
    #[fail(display = "error deserializing event data: {}", _0)]
    Deserialize(#[cause] rmps::decode::Error),
}

impl_from! { for LoadEventError ;
    rmps::decode::Error => |e| LoadEventError::Deserialize(e),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Assigned {
    /// User who assigned.
    pub who: i32,
    /// Module to which the user was assigned.
    pub module: Uuid,
}

/// A draft has reached the final step of an editing process and become a new
/// version of a module.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProcessEnded {
    /// Module of which the draft become a new version.
    pub module: Uuid,
    /// Version which the draft become.
    pub version: i32,
}

/// A slot was filled with a user.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SlotFilled {
    /// Slot which was filled.
    pub slot: i32,
    /// Draft in which the slot was filled.
    pub module: Uuid,
    /// Version of the draft when the slot was filled.
    pub document: i32,
}

/// User vacated a slot.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SlotVacated {
    /// Slot which was vacated.
    pub slot: i32,
    /// Draft in which the slot was vacated.
    pub module: Uuid,
    /// Version of the draft when the slot was vacated.
    pub document: i32,
}

/// Draft was advanced to a next step.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DraftAdvanced {
    /// Draft in which the slot was vacated.
    pub module: Uuid,
    /// Version of the draft when the slot was vacated.
    pub document: i32,
    /// Step to which the draft was advanced.
    pub step: i32,
    /// User's permissions at this step.
    pub permissions: Vec<SlotPermission>,
}

impl Event {
    pub fn kind(&self) -> &'static str {
        match *self {
            Event::Assigned(_) => "assigned",
            Event::ProcessEnded(_) => "process-ended",
            Event::SlotFilled(_) => "slot-filled",
            Event::SlotVacated(_) => "slot-vacated",
            Event::DraftAdvanced(_) => "draft-advanced",
        }
    }
}

impl_from! { for Event ;
    Assigned => |e| Event::Assigned(e),
    ProcessEnded => |e| Event::ProcessEnded(e),
    SlotFilled => |e| Event::SlotFilled(e),
    SlotVacated => |e| Event::SlotVacated(e),
    DraftAdvanced => |e| Event::DraftAdvanced(e),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Group {
    Assigned,
    ProcessEnded,
    SlotAssignment,
    DraftAdvanced,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Assigned,
    ProcessEnded,
    SlotFilled,
    SlotVacated,
    DraftAdvanced,
    Other,
}

impl Kind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "assigned" => Kind::Assigned,
            "process-ended" => Kind::ProcessEnded,
            "slot-filled" => Kind::SlotFilled,
            "slot-vacated" => Kind::SlotVacated,
            "draft-advanced" => Kind::DraftAdvanced,
            _ => Kind::Other,
        }
    }

    pub fn group(&self) -> Group {
        match *self {
            Kind::Assigned => Group::Assigned,
            Kind::ProcessEnded => Group::ProcessEnded,
            Kind::SlotFilled | Kind::SlotVacated => Group::SlotAssignment,
            Kind::DraftAdvanced => Group::DraftAdvanced,
            Kind::Other => Group::Other,
        }
    }
}

/// A version of [`Event`] expanded to include additional information.
///
/// This enum is intended to be used where obtaining additional information
/// about an event would be difficult, for example inside an email template.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExpandedEvent {
    Assigned {
        who: ExpandedUser,
        module: ExpandedModule,
        book: ExpandedBooks,
    },
    ProcessEnded {
        module: ExpandedModule,
        version: i32,
    },
    SlotFilled {
        module: ExpandedModule,
        slot: ExpandedSlot,
    },
    SlotVacated {
        module: ExpandedModule,
        slot: ExpandedSlot,
    },
    DraftAdvanced {
        module: ExpandedModule,
        step: ExpandedStep,
        book: ExpandedBooks,
    },
}

#[derive(Debug, Serialize)]
pub struct ExpandedUser {
    /// User's name.
    pub name: String,
    /// User's profile URL.
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ExpandedModule {
    /// Module's title.
    pub title: String,
    /// Module's URL.
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ExpandedBooks {
    /// One book's title.
    pub title: Option<String>,
    /// One book's URL.
    pub url: Option<String>,
    /// Number of books.
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct ExpandedSlot {
    /// Slot's name.
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ExpandedStep {
    /// Step's name.
    pub name: String,
}

pub fn expand_event(config: &Config, dbcon: &Connection, event: &db::Event)
-> Result<ExpandedEvent, Error> {
    match Kind::from_str(&event.kind) {
        Kind::Assigned =>
            expand_assigned(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::ProcessEnded =>
            expand_process_ended(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::SlotFilled =>
            expand_slot_filled(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::SlotVacated =>
            expand_slot_vacated(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::DraftAdvanced =>
            expand_draft_advanced(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::Other => Err(Error::UnknownEvent(event.kind.clone())),
    }
}

fn expand_books_containing(config: &Config, dbcon: &Connection, module: &Module)
-> Result<ExpandedBooks, Error> {
    let books = module.get_books(dbcon)?;
    let (title, url) = match books.first() {
        None => (None, None),
        Some(id) => {
            let book = match Book::by_id(dbcon, *id){
                Ok(book) => book,
                Err(FindBookError::Database(err)) =>
                    return Err(err.into()),
                Err(FindBookError::NotFound) => panic!(
                    "Inconsistent database: book doesn't exist \
                    but is referenced by an event"),
            }.into_db();

            (
                Some(book.title),
                Some(format!("https://{}/books/{}",
                    config.server.domain, book.id)),
            )
        }
    };

    Ok(ExpandedBooks {
        title,
        url,
        count: books.len(),
    })
}

fn expand_assigned(config: &Config, dbcon: &Connection, ev: Assigned)
-> Result<ExpandedEvent, Error> {
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
    };

    let book = expand_books_containing(config, dbcon, &module)?;
    let module = module.into_db();

    Ok(ExpandedEvent::Assigned {
        who: ExpandedUser {
            name: who.0.name,
            url: format!("https://{}/users/{}",
                config.server.domain, who.0.id),
        },
        module: ExpandedModule {
            title: module.1.title,
            url: format!("https://{}/modules/{}",
                config.server.domain, module.0.id),
        },
        book,
    })
}

fn expand_process_ended(config: &Config, dbcon: &Connection, ev: ProcessEnded)
-> Result<ExpandedEvent, Error> {
    let module = match Module::by_id(dbcon, ev.module) {
        Ok(module) => module,
        Err(FindModuleError::Database(err)) =>
            return Err(err.into()),
        Err(FindModuleError::NotFound) => panic!(
            "Inconsistent database: module doesn't exist \
            but is referenced by an event",
        ),
    }.into_db();

    Ok(ExpandedEvent::ProcessEnded {
        module: ExpandedModule {
            title: module.1.title,
            url: format!("https://{}/modules/{}",
                config.server.domain, module.0.id),
        },
        version: ev.version,
    })
}

fn expand_slot_filled(config: &Config, dbcon: &Connection, ev: SlotFilled)
-> Result<ExpandedEvent, Error> {
    let module = match Module::by_id(dbcon, ev.module) {
        Ok(module) => module,
        Err(FindModuleError::Database(err)) =>
            return Err(err.into()),
        Err(FindModuleError::NotFound) => panic!(
            "Inconsistent database: module doesn't exist \
            but is referenced by an event",
        ),
    }.into_db();

    let slot = match Slot::by_id(dbcon, ev.slot) {
        Ok(module) => module,
        Err(FindSlotError::Database(err)) =>
            return Err(err.into()),
        Err(FindSlotError::NotFound) => panic!(
            "Inconsistent database: slot doesn't exist \
            but is referenced by an event",
        ),
    }.into_db();

    Ok(ExpandedEvent::SlotFilled {
        module: ExpandedModule {
            title: module.1.title,
            url: format!("https://{}/modules/{}",
                config.server.domain, module.0.id),
        },
        slot: ExpandedSlot {
            name: slot.name,
        },
    })
}

fn expand_slot_vacated(config: &Config, dbcon: &Connection, ev: SlotVacated)
-> Result<ExpandedEvent, Error> {
    let module = match Module::by_id(dbcon, ev.module) {
        Ok(module) => module,
        Err(FindModuleError::Database(err)) =>
            return Err(err.into()),
        Err(FindModuleError::NotFound) => panic!(
            "Inconsistent database: module doesn't exist \
            but is referenced by an event",
        ),
    }.into_db();

    let slot = match Slot::by_id(dbcon, ev.slot) {
        Ok(module) => module,
        Err(FindSlotError::Database(err)) =>
            return Err(err.into()),
        Err(FindSlotError::NotFound) => panic!(
            "Inconsistent database: slot doesn't exist \
            but is referenced by an event",
        ),
    }.into_db();

    Ok(ExpandedEvent::SlotVacated {
        module: ExpandedModule {
            title: module.1.title,
            url: format!("https://{}/modules/{}",
                config.server.domain, module.0.id),
        },
        slot: ExpandedSlot {
            name: slot.name,
        },
    })
}

fn expand_draft_advanced(config: &Config, dbcon: &Connection, ev: DraftAdvanced)
-> Result<ExpandedEvent, Error> {
    let module = match Module::by_id(dbcon, ev.module) {
        Ok(module) => module,
        Err(FindModuleError::Database(err)) =>
            return Err(err.into()),
        Err(FindModuleError::NotFound) => panic!(
            "Inconsistent database: module doesn't exist \
            but is referenced by an event",
        ),
    };

    let book = expand_books_containing(config, dbcon, &module)?;
    let module = module.into_db();
    let step = Step::by_id(dbcon, ev.step)?.into_db();

    Ok(ExpandedEvent::DraftAdvanced {
        module: ExpandedModule {
            title: module.1.title,
            url: format!("https://{}/modules/{}",
                config.server.domain, module.0.id),
        },
        step: ExpandedStep {
            name: step.name,
        },
        book,
    })
}
