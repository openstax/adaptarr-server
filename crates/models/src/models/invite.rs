use adaptarr_error::ApiError;
use adaptarr_i18n::Locale;
use adaptarr_macros::From;
use adaptarr_mail::{Mailer, SendFuture};
use chrono::{Duration, Utc, DateTime};
use diesel::{Connection as _, prelude::*, result::Error as DbError};
use failure::Fail;
use serde::Serialize;

use crate::{
    audit,
    db::{Connection, models as db, schema::{invites, users}},
    permissions::PermissionBits,
};
use super::{AssertExists, CreateUserError, Model, User, Role};

/// An invitation into the system for a single, specific user.
#[derive(Debug)]
pub struct Invite {
    data: db::Invite,
}

impl Invite {
    /// Create a new invitation for a given email address.
    pub fn create(dbcon: &Connection, email: &str, role: Option<&Role>)
    -> Result<Invite, CreateInviteError> {
        dbcon.transaction(|| {
            let user = users::table
                .filter(users::email.eq(email))
                .get_result::<db::User>(dbcon)
                .optional()?;

            if user.is_some() {
                return Err(CreateInviteError::UserExists);
            }

            let data = diesel::insert_into(invites::table)
                .values(db::NewInvite {
                    email,
                    expires: Utc::now() + Duration::days(7),
                    role: role.map(Model::id),
                })
                .get_result::<db::Invite>(dbcon)?;

            audit::log_db(dbcon, "invites", data.id, "create", LogNewInvite {
                email,
                expires: data.expires,
                role: data.role,
            });

            Ok(Invite { data })
        })
    }

    /// Get an existing invite by code.
    pub fn from_code(
        dbconn: &Connection,
        secret: &[u8],
        code: &str,
    ) -> Result<Invite, FromCodeError> {
        let mut data = base64::decode_config(code, base64::URL_SAFE_NO_PAD)?;
        let id: i32 = adaptarr_util::unseal(secret, &mut data)?;

        let invite = invites::table
            .filter(invites::id.eq(id))
            .get_result::<db::Invite>(dbconn)
            .optional()?
            .ok_or(FromCodeError::Expired)?;

        if Utc::now() > invite.expires {
            Err(FromCodeError::Expired)
        } else {
            Ok(Invite { data: invite })
        }
    }

    /// Get a secure code representing this invitation.
    pub fn get_code(&self, secret: &[u8]) -> String {
        let sealed = adaptarr_util::seal(secret, self.data.id)
            .expect("failed to seal value");
        base64::encode_config(&sealed, base64::URL_SAFE_NO_PAD)
    }

    /// Fulfil an invitation by creating a user.
    pub fn fulfil(
        self,
        dbconn: &Connection,
        name: &str,
        password: &str,
        language: &str,
    ) -> Result<User, CreateUserError> {
        let role = self.data.role.map(|id|
            Role::by_id(dbconn, id).assert_exists()
        ).transpose()?;

        let user = User::create(
            dbconn,
            None,
            &self.email,
            name,
            password,
            false,
            language,
            PermissionBits::normal(),
            role.as_ref(),
        )?;

        audit::log_db_actor(dbconn, user.id, "invites", self.id, "fulfil", ());

        Ok(user)
    }

    pub fn send_mail(&self, url: &str, locale: &'static Locale) -> SendFuture {
        Mailer::send(
            self.email.as_str(),
            "invite",
            "mail-invite-subject",
            InviteMailArgs {
                url,
                email: &self.email,
            },
            locale,
        )
    }

    pub fn do_send_mail(&self, url: &str, locale: &'static Locale) {
        Mailer::do_send(
            self.email.as_str(),
            "invite",
            "mail-invite-subject",
            InviteMailArgs {
                url,
                email: &self.email,
            },
            locale,
        );
    }
}

impl std::ops::Deref for Invite {
    type Target = db::Invite;

    fn deref(&self) -> &db::Invite {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum CreateInviteError {
    /// There is already an account associated with the email address given.
    #[fail(display = "User exists")]
    #[api(code = "user:new:exists", status = "BAD_REQUEST")]
    UserExists,
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
}

#[derive(ApiError, Debug, Fail, From)]
pub enum FromCodeError {
    /// Invitation has expired or was already used.
    #[fail(display = "Invitation expired")]
    #[api(code = "invitation:expired", status = "BAD_REQUEST")]
    Expired,
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// Code was not valid base64.
    #[fail(display = "Invalid base64: {}", _0)]
    #[api(code = "invitation:invalid", status = "BAD_REQUEST")]
    Decoding(#[cause] #[from] base64::DecodeError),
    /// Code could not be unsealed.
    #[fail(display = "Unsealing error: {}", _0)]
    #[api(code = "invitation:invalid", status = "BAD_REQUEST")]
    Unsealing(#[cause] #[from] adaptarr_util::UnsealingError),
}

/// Arguments for `mail/invite`.
#[derive(Serialize)]
struct InviteMailArgs<'a> {
    /// Registration URL.
    url: &'a str,
    /// Email address which was invited.
    email: &'a str,
}

#[derive(Serialize)]
struct LogNewInvite<'a> {
   email: &'a str,
   expires: DateTime<Utc>,
   role: Option<i32>,
}
