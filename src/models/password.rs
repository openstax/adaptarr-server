use actix_web::{HttpResponse, ResponseError};
use chrono::{Duration, Utc};
use diesel::{
    prelude::*,
    result::Error as DbError,
};

use crate::{
    Config,
    db::{
        Connection,
        models as db,
        schema::password_reset_tokens as tokens,
    },
    utils,
};
use super::user::{User, ChangePasswordError, FindUserError};

/// A token allowing a particular user to change their password once without
/// having to log-in first.
#[derive(Debug)]
pub struct PasswordResetToken {
    data: db::PasswordResetToken,
}

impl PasswordResetToken {
    /// Create a new password reset token for a given user.
    pub fn create(dbcon: &Connection, user: &User) -> Result<PasswordResetToken, CreateTokenError> {
        diesel::insert_into(tokens::table)
            .values(db::NewPasswordResetToken {
                user: user.id,
                expires: Utc::now().naive_utc() + Duration::minutes(15),
            })
            .get_result::<db::PasswordResetToken>(dbcon)
            .map(|data| PasswordResetToken { data })
            .map_err(CreateTokenError)
    }

    /// Get an existing password reset token by code.
    pub fn from_code(dbcon: &Connection, cfg: &Config, code: &str)
    -> Result<PasswordResetToken, FromCodeError> {
        let mut data = base64::decode_config(code, base64::URL_SAFE_NO_PAD)?;
        let id: i32 = utils::unseal(&cfg.server.secret, &mut data)?;

        let token = tokens::table
            .filter(tokens::id.eq(id))
            .get_result::<db::PasswordResetToken>(dbcon)
            .optional()?
            .ok_or(FromCodeError::Expired)?;

        if Utc::now().naive_utc() > token.expires {
            Err(FromCodeError::Expired)
        } else {
            Ok(PasswordResetToken { data: token })
        }
    }

    /// Get a secure code representing this reset token.
    pub fn get_code(&self, cfg: &Config) -> String {
        let sealed = utils::seal(&cfg.server.secret, self.data.id)
            .expect("failed to seal value");
        base64::encode_config(&sealed, base64::URL_SAFE_NO_PAD)
    }

    /// Fulfil this reset token by changing user's password.
    pub fn fulfil(self, dbcon: &Connection, password: &str)
    -> Result<User, ResetPasswordError> {
        let mut user = User::by_id(dbcon, self.data.user)?;
        user.change_password(dbcon, password)?;
        Ok(user)
    }
}

#[derive(Debug, Fail)]
#[fail(display = "Cannot create reset token: {}", _0)]
pub struct CreateTokenError(#[cause] DbError);

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

#[derive(Debug, Fail)]
pub enum ResetPasswordError {
    /// Internal database error.
    #[fail(display = "Database error: {}", _0)]
    Internal(#[cause] DbError),
}

impl_from! { for ResetPasswordError ;
    ChangePasswordError => |e| match e {
        ChangePasswordError::Internal(e) => ResetPasswordError::Internal(e),
    },
    DbError => |e| ResetPasswordError::Internal(e),
    FindUserError => |e| match e {
        FindUserError::Internal(e) => ResetPasswordError::Internal(e),
        FindUserError::NotFound => panic!("Inconsistent database: no user for reset token"),
    },
}