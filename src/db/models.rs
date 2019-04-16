use chrono::NaiveDateTime;
use uuid::Uuid;

use super::schema::*;

#[derive(Associations, Clone, Debug, Identifiable, Queryable)]
pub struct User {
    pub id: i32,
    /// User's email address. We use this for identification (e.g. when logging
    /// into the system) and communication.
    pub email: String,
    /// User's display name. This is visible to other users.
    pub name: String,
    /// Hash of password, currently Argon2.
    pub password: Vec<u8>,
    /// Salt used for hashing password.
    pub salt: Vec<u8>,
    /// Is this user an administrator?
    pub is_super: bool,
    /// User's preferred language
    pub language: String,
    /// Permissions this user has.
    pub permissions: i32,
    /// ID of this user's role.
    pub role: Option<i32>,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "users"]
pub struct NewUser<'a> {
    pub name: &'a str,
    pub email: &'a str,
    pub password: &'a [u8],
    pub salt: &'a [u8],
    pub is_super: bool,
    pub language: &'a str,
    pub permissions: i32,
}

#[derive(AsChangeset, Clone, Copy, Debug)]
#[table_name = "users"]
pub struct PasswordChange<'a> {
    pub password: &'a [u8],
    pub salt: &'a [u8],
}

#[derive(Associations, Clone, Copy, Debug, Identifiable, Queryable)]
#[belongs_to(User, foreign_key = "user")]
pub struct Session {
    /// ID of this session.
    pub id: i32,
    /// ID of the user owning this session.
    pub user: i32,
    /// Maximum age for the session, after which it must not be used.
    pub expires: NaiveDateTime,
    /// Date of the last use of a session. Sessions which were not used for some
    /// time should expire, even if they are still valid according to `expires`.
    pub last_used: NaiveDateTime,
    /// If this an elevated session? To limit attack surface elevated sessions
    /// are granted for a short time, after which they become normal sessions
    /// again.
    pub is_elevated: bool,
    /// Permissions this session has.
    pub permissions: i32,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "sessions"]
pub struct NewSession {
    pub user: i32,
    pub expires: NaiveDateTime,
    pub last_used: NaiveDateTime,
    pub is_elevated: bool,
    pub permissions: i32,
}

#[derive(Clone, Debug, Identifiable, Queryable)]
pub struct Invite {
    /// ID of this invitation.
    pub id: i32,
    /// Email address this invitation is for.
    pub email: String,
    /// Date by which this invitation becomes unusable.
    pub expires: NaiveDateTime,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "invites"]
pub struct NewInvite<'s> {
    pub email: &'s str,
    pub expires: NaiveDateTime,
}

#[derive(Clone, Copy, Debug, Identifiable, Queryable)]
pub struct PasswordResetToken {
    /// ID of this reset token.
    pub id: i32,
    /// ID of the user for whom this token is valid.
    pub user: i32,
    /// Date by which this token becomes unusable.
    pub expires: NaiveDateTime,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "password_reset_tokens"]
pub struct NewPasswordResetToken {
    /// ID of the user for whom this token is valid.
    pub user: i32,
    /// Date by which this token becomes unusable.
    pub expires: NaiveDateTime,
}

#[derive(Clone, Debug, Identifiable, Queryable)]
pub struct File {
    /// ID of this file.
    pub id: i32,
    /// Mime type of this file.
    pub mime: String,
    /// Path to file in the underlying storage containing contents of this file.
    pub path: String,
    /// Has of this file's contents.
    pub hash: Vec<u8>,
}

#[derive(Clone, Debug, Insertable)]
#[table_name = "files"]
pub struct NewFile<'a> {
    pub mime: &'a str,
    pub path: &'a str,
    pub hash: &'a [u8],
}

#[derive(Clone, Debug, Identifiable, Queryable)]
pub struct Document {
    /// ID of this document.
    pub id: i32,
    /// Name of this document.
    pub title: String,
    /// ID of file serving as this document's `index.cnxml`.
    pub index: i32,
    /// Whether a list of possible cross-reference targets has been generated
    /// for this document.
    pub xrefs_ready: bool,
    /// This document's language.
    pub language: String,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "documents"]
pub struct NewDocument<'a> {
    pub title: &'a str,
    pub index: i32,
    pub language: &'a str,
}

#[derive(Clone, Debug, Identifiable, Queryable)]
pub struct DocumentFile {
    /// ID of this document file.
    pub id: i32,
    /// ID of the document this file is a part of.
    pub document: i32,
    /// Name of this file.
    pub name: String,
    /// The actual file.
    pub file: i32,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "document_files"]
pub struct NewDocumentFile<'a> {
    pub document: i32,
    pub name: &'a str,
    pub file: i32,
}

#[derive(Clone, Copy, Debug, Identifiable, Insertable, Queryable)]
pub struct Module {
    /// ID of this module.
    pub id: Uuid,
    /// Document which is the current content of this module.
    pub document: i32,
    /// User assigned to this module.
    pub assignee: Option<i32>,
}

#[derive(Clone, Copy, Debug, Insertable, Queryable)]
pub struct ModuleVersion {
    /// ID of the module.
    pub module: Uuid,
    /// ID of the document which was content of the module at this version.
    pub document: i32,
    /// Date this version was created.
    pub version: NaiveDateTime,
}

#[derive(Clone, Debug, Identifiable, Queryable)]
pub struct Book {
    /// ID of this book.
    pub id: Uuid,
    /// Title of this book.
    pub title: String,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "books"]
pub struct NewBook<'a> {
    pub id: Uuid,
    pub title: &'a str,
}

#[derive(Clone, Debug, Identifiable, Queryable)]
#[primary_key(book, id)]
pub struct BookPart {
    /// ID of the book this is a part of.
    pub book: Uuid,
    /// ID of this part within `book`.
    pub id: i32,
    /// Title of this part.
    pub title: String,
    /// If this field is `Some` this part is a module. Otherwise it is a group
    /// of book parts.
    pub module: Option<Uuid>,
    /// ID of a book part this book part is an item in.
    ///
    /// As a special case, this field is 0 for group 0.
    pub parent: i32,
    /// Index of this book part within `parent`.
    pub index: i32,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "book_parts"]
pub struct NewBookPart<'a> {
    pub book: Uuid,
    pub title: &'a str,
    pub module: Option<Uuid>,
    pub parent: i32,
    pub index: i32,
}

#[derive(Clone, Copy, Debug, AsChangeset)]
#[table_name = "book_parts"]
pub struct NewBookPartLocation {
    pub book: Uuid,
    pub parent: i32,
    pub index: i32,
}

#[derive(Clone, Copy, Debug, Identifiable, Insertable, Queryable)]
#[primary_key(module, user)]
pub struct Draft {
    /// Module of which this is a draft.
    pub module: Uuid,
    /// User owning this draft.
    pub user: i32,
    /// Contents of this draft.
    pub document: i32,
}

#[derive(Clone, Debug, Identifiable, Queryable)]
pub struct Event {
    /// ID of this event.
    pub id: i32,
    /// ID of the user for which this event was generated.
    pub user: i32,
    /// Time at which this event was generated.
    pub timestamp: NaiveDateTime,
    /// Short string describing what kind of event is this.
    pub kind: String,
    /// True if the user has not yet reviewed this event.
    pub is_unread: bool,
    /// Actual data for the event, serialized as MessagePack.
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "events"]
pub struct NewEvent<'a> {
    pub user: i32,
    pub kind: &'a str,
    pub data: &'a [u8],
}

#[derive(Clone, Debug, Identifiable, Queryable)]
#[primary_key(document, element)]
pub struct XrefTarget {
    /// ID of the document in this this element exists.
    pub document: i32,
    /// ID of the element.
    ///
    /// Note that this is an XML ID, not a database ID.
    pub element: String,
    /// Type of this element.
    pub type_: String,
    /// A short description of this element intended to make it easier for users
    /// to select the correct element when creating a cross-document reference.
    ///
    /// This field may be `None` if it was not possible to generate
    /// a description.
    pub description: Option<String>,
    /// ID of a reference target “owning” this one.
    ///
    /// This field is used for elements such as subfigures or exercise
    /// solutions, to refer to their parent (figure or exercise in those cases),
    /// so that they can be better grouped in a selection UI.
    pub context: Option<String>,
    /// Value of the type-counter at this element.
    ///
    /// For elements that have `context` type counter resets at context.
    pub counter: i32,
}

#[derive(AsChangeset, Clone, Copy, Debug, Insertable)]
#[table_name = "xref_targets"]
pub struct NewXrefTarget<'s> {
    pub document: i32,
    pub element: &'s str,
    pub type_: &'s str,
    pub description: Option<&'s str>,
    pub context: Option<&'s str>,
    pub counter: i32,
}

#[derive(Clone, Debug, Identifiable, Queryable)]
pub struct Role {
    /// ID of this role.
    pub id: i32,
    /// Name of this role.
    pub name: String,
    /// Additional permissions a user has when they are a member of this role.
    pub permissions: i32,
}

#[derive(AsChangeset, Clone, Copy, Debug, Insertable)]
#[table_name = "roles"]
pub struct NewRole<'s> {
    pub name: &'s str,
    pub permissions: i32,
}

/// Root model of an editing process.
///
/// Editing processes are versioned, the actual implementation of an editing
/// process is described by the [`EditProcessVersion`] model. The actual version
/// is the newest [`EditProcessVersion`], according to its `version` field.
#[derive(Clone, Debug, Identifiable, Queryable)]
#[table_name = "edit_processes"]
pub struct EditProcess {
    /// Process's ID.
    pub id: i32,
    /// Process's name.
    pub name: String,
}

#[derive(AsChangeset, Clone, Debug, Insertable)]
#[table_name = "edit_processes"]
pub struct NewEditProcess<'a> {
    pub name: &'a str,
}

/// Actual implementation of an editing process.
#[derive(Associations, Clone, Debug, Identifiable, Queryable)]
#[belongs_to(EditProcess, foreign_key = "process")]
pub struct EditProcessVersion {
    /// Version's ID.
    pub id: i32,
    /// Process's ID.
    pub process: i32,
    /// Date of last modification.
    pub version: NaiveDateTime,
    /// Initial step.
    pub start: i32,
}

#[derive(AsChangeset, Clone, Copy, Insertable)]
#[table_name = "edit_process_versions"]
pub struct NewEditProcessVersion {
    pub process: i32,
    pub version: NaiveDateTime,
    pub start: i32,
}

/// A “seat” occupied per document by one of the users involved in editing that
/// document.
#[derive(Associations, Clone, Debug, Eq, Identifiable, PartialEq, Queryable)]
#[belongs_to(EditProcessVersion, foreign_key = "process")]
pub struct EditProcessSlot {
    /// Slot's ID.
    pub id: i32,
    /// Parent process version's ID.
    pub process: i32,
    /// Slot's name.
    pub name: String,
    /// When set, role to which this slot is limited.
    pub role: Option<i32>,
    /// Whether the system should automatically fill this slot with a user.
    pub autofill: bool,
}

#[derive(AsChangeset, Clone, Copy, Debug, Insertable)]
#[table_name = "edit_process_slots"]
pub struct NewEditProcessSlot<'a> {
    pub process: i32,
    pub name: &'a str,
    pub role: Option<i32>,
    pub autofill: bool,
}

/// A single editing step.
#[derive(Associations, Clone, Debug, Identifiable, Queryable)]
#[belongs_to(EditProcessVersion, foreign_key = "process")]
pub struct EditProcessStep {
    /// Step's ID.
    pub id: i32,
    /// Parent process version's ID.
    pub process: i32,
    /// Step's name.
    ///
    /// This name is used to identify a step when editing a process, and when
    /// displaying a module's status.
    pub name: String,
}

#[derive(AsChangeset, Clone, Copy, Debug, Insertable)]
#[table_name = "edit_process_steps"]
pub struct NewEditProcessStep<'a> {
    pub process: i32,
    pub name: &'a str,
}

/// List of slots assigned to a document at a given editing step.
#[derive(Associations, Clone, Copy, Debug, Identifiable, Insertable, Queryable)]
#[primary_key(step, slot, permission)]
#[belongs_to(EditProcessStep, foreign_key = "step")]
pub struct EditProcessStepSlot {
    /// Step's ID.
    pub step: i32,
    /// Slot's ID.
    pub slot: i32,
    /// Slot's permission.
    pub permission: super::types::SlotPermission,
}

/// Possible transition between editing steps.
#[derive(Clone, Debug, Identifiable, Queryable)]
#[primary_key(from, to)]
pub struct EditProcessLink {
    /// Source step's ID.
    pub from: i32,
    /// Destination step's ID.
    pub to: i32,
    /// Link's name.
    ///
    /// This name is displayed in UI as an action changing module's current
    /// step.
    pub name: String,
    /// ID of slot allowed to change modules step.
    pub slot: i32,
}

#[derive(AsChangeset, Clone, Copy, Debug, Insertable)]
#[table_name = "edit_process_links"]
pub struct NewEditProcessLink<'a> {
    pub from: i32,
    pub to: i32,
    pub name: &'a str,
    pub slot: i32,
}
