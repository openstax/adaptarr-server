use actix_web::{HttpResponse, ResponseError};
use chrono::{Duration, Utc};
use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};

use crate::{
    Config,
    db::{
        Connection,
        models as db,
        schema::{invites, users},
    },
    utils,
};

/// An invitation into the system for a single, specific user.
#[derive(Debug)]
pub struct Invite {
    data: db::Invite,
}

impl Invite {
    /// Create a new invitation for a given email address.
    pub fn create(dbcon: &Connection, email: &str) -> Result<Invite, CreateInviteError> {
        dbcon.transaction(|| {
            let user = users::table
                .filter(users::email.eq(email))
                .get_result::<db::User>(dbcon)
                .optional()?;

            if user.is_some() {
                return Err(CreateInviteError::UserExists);
            }

            diesel::insert_into(invites::table)
                .values(db::NewInvite {
                    email: email,
                    expires: Utc::now().naive_utc() + Duration::days(7),
                })
                .get_result::<db::Invite>(dbcon)
                .map(|data| Invite { data })
                .map_err(Into::into)
        })
    }

    /// Get an existing invite by code.
    pub fn from_code(
        dbconn: &Connection,
        cfg: &Config,
        code: &str,
    ) -> Result<Invite, FromCodeError> {
        let mut data = base64::decode_config(code, base64::URL_SAFE_NO_PAD)?;
        let id: i32 = utils::unseal(&cfg.server.secret, &mut data)?;

        let invite = invites::table
            .filter(invites::id.eq(id))
            .get_result::<db::Invite>(dbconn)
            .optional()?
            .ok_or(FromCodeError::Expired)?;

        if Utc::now().naive_utc() > invite.expires {
            Err(FromCodeError::Expired)
        } else {
            Ok(Invite { data: invite })
        }
    }

    /// Get a secure code representing this invitation.
    pub fn get_code(&self, cfg: &Config) -> String {
        let sealed = utils::seal(&cfg.server.secret, self.data.id)
            .expect("failed to seal value");
        base64::encode_config(&sealed, base64::URL_SAFE_NO_PAD)
    }
}

impl std::ops::Deref for Invite {
    type Target = db::Invite;

    fn deref(&self) -> &db::Invite {
        &self.data
    }
}

#[derive(Debug, Fail)]
pub enum CreateInviteError {
    /// There is already an account associated with the email address given.
    #[fail(display = "User exists")]
    UserExists,
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
}

impl_from! { for CreateInviteError ;
    DbError => |e| CreateInviteError::Database(e),
}

impl ResponseError for CreateInviteError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            CreateInviteError::UserExists =>
                HttpResponse::BadRequest().body("user exists"),
            CreateInviteError::Database(_) =>
                HttpResponse::InternalServerError().finish(),
        }
    }
}

#[derive(Debug, Fail)]
pub enum FromCodeError {
    /// Invitation has expired or was already used.
    #[fail(display = "Invitation expired")]
    Expired,
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// Code was not valid base64.
    #[fail(display = "Invalid base64: {}", _0)]
    Decoding(#[cause] base64::DecodeError),
    /// Code could not be unsealed.
    #[fail(display = "Unsealing error: {}", _0)]
    Unsealing(#[cause] utils::UnsealingError),
}

impl_from! { for FromCodeError ;
    DbError => |e| FromCodeError::Database(e),
    base64::DecodeError => |e| FromCodeError::Decoding(e),
    utils::UnsealingError => |e| FromCodeError::Unsealing(e),
}

impl ResponseError for FromCodeError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            FromCodeError::Expired =>
                HttpResponse::BadRequest().body("invitation expired"),
            FromCodeError::Database(_) =>
                HttpResponse::InternalServerError().finish(),
            FromCodeError::Decoding(_) | FromCodeError::Unsealing(_) =>
                HttpResponse::BadRequest().body("invalid invitation code"),
        }
    }
}
