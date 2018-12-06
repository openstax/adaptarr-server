use chrono::NaiveDateTime;

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
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "users"]
pub struct NewUser<'a> {
    pub name: &'a str,
    pub email: &'a str,
    pub password: &'a [u8],
    pub salt: &'a [u8],
    pub is_super: bool,
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
    /// If this an administrative session? To limit attack surface
    /// administrative sessions are granted for a short time, after which they
    /// become normal sessions again.
    pub is_super: bool,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "sessions"]
pub struct NewSession {
    pub user: i32,
    pub expires: NaiveDateTime,
    pub last_used: NaiveDateTime,
    pub is_super: bool,
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
    pub name: String,
    /// ID of file serving as this document's `index.cnxml`.
    pub index: i32,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "documents"]
pub struct NewDocument<'a> {
    pub name: &'a str,
    pub index: i32,
}
