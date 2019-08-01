use adaptarr_macros::From;
use failure::Fail;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
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
        conversation::{
            Event as ConversationEvent,
            FindEventError,
            format::{self as message_format, Format},
        },
        editing::{
            Step,
            slot::{Slot, FindSlotError},
        },
        module::{Module, FindModuleError},
        user::{User, FindUserError},
    },
};
use super::Error;

#[derive(Clone, Debug, Deserialize, Serialize, From)]
#[serde(untagged)]
pub enum Event {
    Assigned(#[from] Assigned),
    ProcessEnded(#[from] ProcessEnded),
    ProcessCancelled(#[from] ProcessCancelled),
    SlotFilled(#[from] SlotFilled),
    SlotVacated(#[from] SlotVacated),
    DraftAdvanced(#[from] DraftAdvanced),
    NewMessage(#[from] NewMessage),
}

impl Event {
    pub fn load(kind: &str, data: &[u8]) -> Result<Event, LoadEventError> {
        match Kind::from_str(&kind) {
            Kind::Assigned =>
                Ok(Event::Assigned(rmps::from_slice(&data)?)),
            Kind::ProcessEnded =>
                Ok(Event::ProcessEnded(rmps::from_slice(&data)?)),
            Kind::ProcessCancelled =>
                Ok(Event::ProcessCancelled(rmps::from_slice(&data)?)),
            Kind::SlotFilled =>
                Ok(Event::SlotFilled(rmps::from_slice(&data)?)),
            Kind::SlotVacated =>
                Ok(Event::SlotVacated(rmps::from_slice(&data)?)),
            Kind::DraftAdvanced =>
                Ok(Event::DraftAdvanced(rmps::from_slice(&data)?)),
            Kind::NewMessage =>
                Ok(Event::NewMessage(rmps::from_slice(&data)?)),
            Kind::Other => Err(LoadEventError::UnknownEvent(kind.to_string())),
        }
    }
}

#[derive(Debug, Fail, From)]
pub enum LoadEventError {
    #[fail(display = "unknown event type: {}", _0)]
    UnknownEvent(String),
    #[fail(display = "error deserializing event data: {}", _0)]
    Deserialize(#[cause] #[from] rmps::decode::Error),
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

/// Editing process for a draft was cancelled.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProcessCancelled {
    /// Module for a draft of which the editing process was cancelled.
    pub module: Uuid,
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

/// A new message was sent in a conversation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewMessage {
    /// Conversation in which this message was sent.
    pub conversation: i32,
    /// Author of the message.
    pub author: i32,
    /// ID of the message.
    pub message: i32,
}

impl Event {
    pub fn kind(&self) -> &'static str {
        match *self {
            Event::Assigned(_) => "assigned",
            Event::ProcessEnded(_) => "process-ended",
            Event::ProcessCancelled(_) => "process-cancelled",
            Event::SlotFilled(_) => "slot-filled",
            Event::SlotVacated(_) => "slot-vacated",
            Event::DraftAdvanced(_) => "draft-advanced",
            Event::NewMessage(_) => "new-message",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Group {
    Assigned,
    ProcessEnded,
    SlotAssignment,
    DraftAdvanced,
    Conversation,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Assigned,
    ProcessEnded,
    ProcessCancelled,
    SlotFilled,
    SlotVacated,
    DraftAdvanced,
    NewMessage,
    Other,
}

impl Kind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "assigned" => Kind::Assigned,
            "process-ended" => Kind::ProcessEnded,
            "process-cancelled" => Kind::ProcessCancelled,
            "slot-filled" => Kind::SlotFilled,
            "slot-vacated" => Kind::SlotVacated,
            "draft-advanced" => Kind::DraftAdvanced,
            "new-message" => Kind::NewMessage,
            _ => Kind::Other,
        }
    }

    pub fn group(self) -> Group {
        match self {
            Kind::Assigned => Group::Assigned,
            Kind::ProcessEnded | Kind::ProcessCancelled => Group::ProcessEnded,
            Kind::SlotFilled | Kind::SlotVacated => Group::SlotAssignment,
            Kind::DraftAdvanced => Group::DraftAdvanced,
            Kind::NewMessage => Group::Conversation,
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
    ProcessCancelled {
        module: ExpandedModule,
    },
    SlotFilled {
        draft: ExpandedDraft,
        slot: ExpandedSlot,
    },
    SlotVacated {
        draft: ExpandedDraft,
        slot: ExpandedSlot,
    },
    DraftAdvanced {
        draft: ExpandedDraft,
        step: ExpandedStep,
        book: ExpandedBooks,
    },
    NewMessage {
        conversation: ExpandedConversation,
        author: ExpandedUser,
        message: ExpandedMessage,
    }
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
pub struct ExpandedDraft {
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

#[derive(Debug, Serialize)]
pub struct ExpandedConversation {
    /// Conversation's URL.
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ExpandedMessage {
    /// Message rendered as plain text with no formatting.
    pub text: String,
    /// Conversation rendered as HTML for email (this differs form a normal
    /// HTML, which can't be used in emails).
    pub html: String,
}

pub fn expand_event(config: &Config, dbcon: &Connection, event: &db::Event)
-> Result<ExpandedEvent, Error> {
    match Kind::from_str(&event.kind) {
        Kind::Assigned =>
            expand_assigned(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::ProcessEnded =>
            expand_process_ended(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::ProcessCancelled =>
            expand_process_cancelled(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::SlotFilled =>
            expand_slot_filled(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::SlotVacated =>
            expand_slot_vacated(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::DraftAdvanced =>
            expand_draft_advanced(config, dbcon, rmps::from_slice(&event.data)?),
        Kind::NewMessage =>
            expand_new_message(config, dbcon, rmps::from_slice(&event.data)?),
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

fn expand_user(config: &Config, dbcon: &Connection, id: i32)
-> Result<ExpandedUser, Error> {
    let user = match User::by_id(dbcon, id) {
        Ok(user) => user,
        Err(FindUserError::Internal(err)) =>
            return Err(err.into()),
        Err(FindUserError::NotFound) => panic!(
            "Inconsistent database: user doesn't exist \
            but is referenced by an event",
        ),
    }.into_db();

    Ok(ExpandedUser {
        name: user.0.name,
        url: format!("https://{}/users/{}",
            config.server.domain, user.0.id),
    })
}

fn expand_assigned(config: &Config, dbcon: &Connection, ev: Assigned)
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

    Ok(ExpandedEvent::Assigned {
        who: expand_user(config, dbcon, ev.who)?,
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

fn expand_process_cancelled(config: &Config, dbcon: &Connection, ev: ProcessCancelled)
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

    Ok(ExpandedEvent::ProcessCancelled {
        module: ExpandedModule {
            title: module.1.title,
            url: format!("https://{}/modules/{}",
                config.server.domain, module.0.id),
        },
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
        draft: ExpandedDraft {
            title: module.1.title,
            url: format!("https://{}/drafts/{}",
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
        draft: ExpandedDraft {
            title: module.1.title,
            url: format!("https://{}/drafts/{}",
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
        draft: ExpandedDraft {
            title: module.1.title,
            url: format!("https://{}/drafts/{}",
                config.server.domain, module.0.id),
        },
        step: ExpandedStep {
            name: step.name,
        },
        book,
    })
}

fn expand_new_message(config: &Config, dbcon: &Connection, ev: NewMessage)
-> Result<ExpandedEvent, Error> {
    let message = match ConversationEvent::by_id(dbcon, ev.message) {
        Ok(event) => event,
        Err(FindEventError::Database(err)) => return Err(err.into()),
        Err(FindEventError::NotFound) => panic!(
            "Inconsistent database: conversation event doesn't exist but is \
            referenced by an event",
        ),
    }.into_db();

    Ok(ExpandedEvent::NewMessage {
        conversation: ExpandedConversation {
            url: format!("https://{}/conversations/{}#{}",
                config.server.domain, ev.conversation, message.id),
        },
        author: expand_user(config, dbcon, ev.author)?,
        message: message_format::render(
            &message.data, MessageRenderer::new(dbcon),
        ).expect("Inconsistent database: conversation contains an invalid \
            message"),
    })
}

struct MessageRenderer<'a> {
    dbcon: &'a Connection,
    text: String,
    html: String,
    format: Vec<Format>,
    first_para: bool,
}

impl<'a> MessageRenderer<'a> {
    fn new(dbcon: &'a Connection) -> Self {
        MessageRenderer {
            dbcon,
            text: String::new(),
            html: String::new(),
            format: Vec::new(),
            first_para: true,
        }
    }
}

impl<'a> message_format::Renderer for MessageRenderer<'a> {
    type Result = ExpandedMessage;

    fn begin_paragraph(&mut self) {
        let top = if self.first_para { "10px" } else { "0" };
        let _ = write!(self.html,
            r#"<tr><td style="padding: {} 14px 10px 14px;">"#, top);
        self.first_para = false;
    }

    fn end_paragraph(&mut self) {
        self.pop_format(Format::all(), Format::empty());
        self.format.clear();

        self.text.push_str("\n\n");
        self.html.push_str("</tr></td>");
    }

    fn text(&mut self, text: &str) {
        self.text.push_str(&text);
        self.html.push_str(&tera::escape_html(&text));
    }

    fn push_format(&mut self, format: Format, _: Format) {
        if format.contains(Format::EMPHASIS) {
            self.format.push(Format::EMPHASIS);
            self.html.push_str("<em>");
        }
        if format.contains(Format::STRONG) {
            self.format.push(Format::STRONG);
            self.html.push_str("<strong>");
        }
    }

    fn pop_format(&mut self, mut format: Format, current: Format) {
        let mut reapply = Format::empty();

        while !format.is_empty() {
            let f = match self.format.pop() {
                Some(f) => f,
                None => break,
            };

            if f == Format::EMPHASIS {
                self.html.push_str("</em>");
            } else if f == Format::STRONG {
                self.html.push_str("</strong>");
            }

            if format.contains(f) {
                format.remove(f);
            } else {
                reapply.insert(f);
            }
        }

        if !reapply.is_empty() {
            self.push_format(reapply, current);
        }
    }

    fn hyperlink(&mut self, label: Option<&str>, url: &str) {
        match label {
            Some(label) => { let _ = write!(self.text, "{} ({})", label, url); }
            None => self.text.push_str(url),
        }

        let url = tera::escape_html(url);

        match label {
            Some(label) => {
                let _ = write!(self.html,
                    r#"<a href="{}" target="_blank" rel="noopener">{}</a>"#,
                    url,
                    tera::escape_html(label),
                );
            }
            None => {
                let _ = write!(self.html,
                    r#"<a href="{0}" target="_blank" rel="noopener">{0}</a>"#,
                    url,
                );
            }
        }
    }

    fn mention(&mut self, user: i32) {
        let user = User::by_id(self.dbcon, user)
            .expect("Inconsistent database: conversation message mentions a \
                non-existent user");
        self.text.push_str(&user.name);
        self.html.push_str(&tera::escape_html(&user.name));
    }

    fn finish(mut self) -> ExpandedMessage {
        let end = self.text.rfind(|c: char| !c.is_whitespace()).map_or(0, |x| x + 1);
        self.text.truncate(end);

        ExpandedMessage {
            text: self.text,
            html: self.html,
        }
    }
}
