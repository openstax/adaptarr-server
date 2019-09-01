use adaptarr_error::ApiError;
use adaptarr_macros::From;
use chrono::{Duration, Utc};
use diesel::{prelude::*, result::Error as DbError};
use failure::Fail;

use crate::{
    audit,
    db::{
        Connection,
        models as db,
        schema::password_reset_tokens as tokens,
    },
};
use super::{AssertExists, Model, User, ChangePasswordError};

/// A token allowing a particular user to change their password once without
/// having to log-in first.
#[derive(Debug)]
pub struct PasswordResetToken {
    data: db::PasswordResetToken,
}

impl PasswordResetToken {
    /// Create a new password reset token for a given user.
    pub fn create(dbcon: &Connection, user: &User)
    -> Result<PasswordResetToken, CreateTokenError> {
        let data = diesel::insert_into(tokens::table)
            .values(db::NewPasswordResetToken {
                user: user.id,
                expires: Utc::now().naive_utc() + Duration::minutes(15),
            })
            .get_result::<db::PasswordResetToken>(dbcon)
            .map_err(CreateTokenError)?;

        audit::log_db_actor(
            dbcon, user.id, "password-reset-tokens", data.id, "create", ());

        Ok(PasswordResetToken { data })
    }

    /// Get an existing password reset token by code.
    pub fn from_code(dbcon: &Connection, secret: &[u8], code: &str)
    -> Result<PasswordResetToken, FromCodeError> {
        let mut data = base64::decode_config(code, base64::URL_SAFE_NO_PAD)?;
        let id: i32 = adaptarr_util::unseal(secret, &mut data)?;

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
    pub fn get_code(&self, secret: &[u8]) -> String {
        let sealed = adaptarr_util::seal(secret, self.data.id)
            .expect("failed to seal value");
        base64::encode_config(&sealed, base64::URL_SAFE_NO_PAD)
    }

    /// Fulfil this reset token by changing user's password.
    pub fn fulfil(self, dbcon: &Connection, password: &str)
    -> Result<User, ResetPasswordError> {
        let mut user = User::by_id(dbcon, self.data.user)
            .assert_exists()?;
        audit::with_actor(
            audit::Actor::User(user.id),
            || user.change_password(dbcon, password),
        )?;
        audit::log_db_actor(
            dbcon,
            user.id,
            "password-reset-tokens",
            self.data.id,
            "fulfil-password-reset-token",
            (),
        );
        Ok(user)
    }
}

#[derive(ApiError, Debug, Fail, From)]
#[fail(display = "Cannot create reset token: {}", _0)]
#[api(internal)]
pub struct CreateTokenError(#[cause] #[from] DbError);

#[derive(ApiError, Debug, Fail, From)]
pub enum FromCodeError {
    /// Reset token has expired or was already used.
    #[fail(display = "Reset token expired")]
    #[api(code = "password:reset:expired", status = "BAD_REQUEST")]
    Expired,
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// Code was not valid base64.
    #[fail(display = "Invalid base64: {}", _0)]
    #[api(code = "password:reset:invalid", status = "BAD_REQUEST")]
    Decoding(#[cause] #[from] base64::DecodeError),
    /// Code could not be unsealed.
    #[fail(display = "Unsealing error: {}", _0)]
    #[api(code = "password:reset:invalid", status = "BAD_REQUEST")]
    Unsealing(#[cause] #[from] adaptarr_util::UnsealingError),
}

#[derive(ApiError, Debug, Fail, From)]
pub enum ResetPasswordError {
    /// Internal database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Internal(#[cause] #[from] DbError),
    #[fail(display = "{}", _0)]
    Password(#[cause] ChangePasswordError),
}

impl From<ChangePasswordError> for ResetPasswordError {
    fn from(e: ChangePasswordError) -> Self {
        match e {
            ChangePasswordError::Internal(e) => ResetPasswordError::Internal(e),
            e => ResetPasswordError::Password(e),
        }
    }
}
