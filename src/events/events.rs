use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::Config,
    db::{
        Connection,
        models as db,
    },
    models::{
        book::{Book, FindBookError},
        editing::slot::{Slot, FindSlotError},
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

impl Event {
    pub fn kind(&self) -> &'static str {
        match *self {
            Event::Assigned(_) => "assigned",
            Event::ProcessEnded(_) => "process-ended",
            Event::SlotFilled(_) => "slot-filled",
            Event::SlotVacated(_) => "slot-vacated",
        }
    }
}

impl_from! { for Event ;
    Assigned => |e| Event::Assigned(e),
    ProcessEnded => |e| Event::ProcessEnded(e),
    SlotFilled => |e| Event::SlotFilled(e),
    SlotVacated => |e| Event::SlotVacated(e),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Group {
    Assigned,
    ProcessEnded,
    SlotAssignment,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Assigned,
    ProcessEnded,
    SlotFilled,
    SlotVacated,
    Other,
}

impl Kind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "assigned" => Kind::Assigned,
            "process-ended" => Kind::ProcessEnded,
            "slot-filled" => Kind::SlotFilled,
            "slot-vacated" => Kind::SlotVacated,
            _ => Kind::Other,
        }
    }

    pub fn group(&self) -> Group {
        match *self {
            Kind::Assigned => Group::Assigned,
            Kind::ProcessEnded => Group::ProcessEnded,
            Kind::SlotFilled | Kind::SlotVacated => Group::SlotAssignment,
            Kind::Other => Group::Other,
        }
    }
}

/// A version of [`Event`] expanded to include additional information.
///
/// This enum is intended to be used where obtaining additional information
/// about an event would be difficult, for example inside an email template.
#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
pub enum ExpandedEvent {
    #[serde(rename = "assigned")]
    Assigned {
        who: ExpandedUser,
        module: ExpandedModule,
        book: ExpandedBooks,
    },
    #[serde(rename = "process-ended")]
    ProcessEnded {
        module: ExpandedModule,
        version: i32,
    },
    #[serde(rename = "slot-filled")]
    SlotFilled {
        module: ExpandedModule,
        slot: ExpandedSlot,
    },
    #[serde(rename = "slot-vacated")]
    SlotVacated {
        module: ExpandedModule,
        slot: ExpandedSlot,
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

pub fn expand_event(config: &Config, dbcon: &Connection, event: &db::Event)
-> Result<ExpandedEvent, Error> {
    let data = rmps::from_slice(&event.data)?;

    match data {
        Event::Assigned(ref ev) => expand_assigned(config, dbcon, ev),
        Event::ProcessEnded(ref ev) => expand_process_ended(config, dbcon, ev),
        Event::SlotFilled(ref ev) => expand_slot_filled(config, dbcon, ev),
        Event::SlotVacated(ref ev) => expand_slot_vacated(config, dbcon, ev),
    }
}

fn expand_assigned(config: &Config, dbcon: &Connection, ev: &Assigned)
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

    let books = module.get_books(dbcon)?;
    let (book_title, book_url) = match books.first() {
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
        book: ExpandedBooks {
            title: book_title,
            url: book_url,
            count: books.len(),
        },
    })
}

fn expand_process_ended(config: &Config, dbcon: &Connection, ev: &ProcessEnded)
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

fn expand_slot_filled(config: &Config, dbcon: &Connection, ev: &SlotFilled)
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

fn expand_slot_vacated(config: &Config, dbcon: &Connection, ev: &SlotVacated)
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
