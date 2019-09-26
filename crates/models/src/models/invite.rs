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
    config::Config,
    db::{Connection, models as db, schema::{invites, users, teams}},
    permissions::{SystemPermissions, TeamPermissions},
};
use super::{
    AssertExists,
    CreateUserError,
    Model,
    Optional,
    Role,
    User,
    team::{AddMemberError, Team},
};

/// An invitation into the system for a single, specific user.
#[derive(Debug)]
pub struct Invite {
    data: db::Invite,
    team: Team,
    user: Option<User>,
}

impl Invite {
    /// Create a new invitation for a given email address.
    pub fn create(
        db: &Connection,
        team: Team,
        email: &str,
        role: Option<&Role>,
        permissions: TeamPermissions,
    ) -> Result<Invite, CreateInviteError> {
        db.transaction(|| {
            let user = User::by_email(db, email).optional()?;

            let data = diesel::insert_into(invites::table)
                .values(db::NewInvite {
                    email,
                    expires: Utc::now() + Duration::days(7),
                    role: role.map(Model::id),
                    team: team.id(),
                    permissions: permissions.bits(),
                    user: user.id(),
                })
                .get_result::<db::Invite>(db)?;

            audit::log_db(db, "invites", data.id, "create", LogNewInvite {
                email,
                expires: data.expires,
                role: data.role,
            });

            Ok(Invite { data, team, user })
        })
    }

    /// Create a new team invitation for an existing user.
    pub fn create_for_existing(
        db: &Connection,
        team: Team,
        role: Option<&Role>,
        permissions: TeamPermissions,
        user: User,
    ) -> Result<Invite, CreateInviteError> {
        db.transaction(|| {
            let data = diesel::insert_into(invites::table)
                .values(db::NewInvite {
                    email: &user.email,
                    expires: Utc::now() + Duration::days(7),
                    role: role.map(Model::id),
                    team: team.id(),
                    permissions: permissions.bits(),
                    user: Some(user.id()),
                })
                .get_result::<db::Invite>(db)?;

            audit::log_db(db, "invites", data.id, "create", LogNewTeamInvite {
                expires: data.expires,
                role: data.role,
                user: user.id,
            });

            Ok(Invite { data, team, user: Some(user) })
        })
    }

    /// Get an existing invite by code.
    pub fn from_code(
        db: &Connection,
        secret: &[u8],
        code: &str,
    ) -> Result<Invite, FromCodeError> {
        let mut data = base64::decode_config(code, base64::URL_SAFE_NO_PAD)?;
        let id: i32 = adaptarr_util::unseal(secret, &mut data)?;

        let (invite, team, user) = invites::table
            .filter(invites::id.eq(id))
            .inner_join(teams::table)
            .left_join(users::table)
            .get_result::<(db::Invite, db::Team, Option<db::User>)>(db)
            .optional()?
            .ok_or(FromCodeError::Expired)?;

        if Utc::now() > invite.expires {
            Err(FromCodeError::Expired)
        } else {
            Ok(Invite {
                data: invite,
                team: Team::from_db(team),
                user: Option::<User>::from_db(user),
            })
        }
    }

    /// Delete this invitation.
    pub fn delete(self, db: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(db)?;
        Ok(())
    }

    /// Get a secure code representing this invitation.
    pub fn get_code(&self, secret: &[u8]) -> String {
        let sealed = adaptarr_util::seal(secret, self.data.id)
            .expect("failed to seal value");
        base64::encode_config(&sealed, base64::URL_SAFE_NO_PAD)
    }

    /// Is this an invitation for an existing user to join a team?
    pub fn is_for_existing(&self) -> bool {
        self.user.is_some()
    }

    /// Get the team to which the user is being invited.
    pub fn team(&self) -> &Team {
        &self.team
    }

    /// Get the user being invited, if they have an account already.
    pub fn user(&self) -> Option<&User> {
        self.user.as_ref()
    }

    /// Fulfil an invitation by creating a user.
    pub fn fulfil_new(
        mut self,
        db: &Connection,
        name: &str,
        password: &str,
        language: &str,
    ) -> Result<User, FulfilInviteError> {
        db.transaction(|| {
            let role = Option::<Role>::by_id(db, self.data.role).assert_exists()?;

            let user = match self.data.user {
                None => User::create(
                    db,
                    None,
                    &self.email,
                    name,
                    password,
                    false,
                    language,
                    SystemPermissions::empty(),
                )?,
                Some(user) => User::by_id(db, user).assert_exists()?,
            };

            let permissions = TeamPermissions::from_bits_truncate(
                self.data.permissions);

            self.team.add_member(db, &user, permissions, role.as_ref())?;
            audit::log_db_actor(db, user.id, "invites", self.id, "fulfil", ());

            diesel::delete(&self.data).execute(db)?;

            Ok(user)
        })
    }

    /// Fulfil an invitation by adding an existing user to a team.
    pub fn fulfil_existing(mut self, db: &Connection)
    -> Result<User, FulfilInviteError> {
        db.transaction(|| {
            let role = Option::<Role>::by_id(db, self.data.role).assert_exists()?;

            let user = match self.data.user {
                None => return Err(FulfilInviteError::NoUser),
                Some(user) => User::by_id(db, user).assert_exists()?,
            };

            let permissions = TeamPermissions::from_bits_truncate(
                self.data.permissions);

            self.team.add_member(db, &user, permissions, role.as_ref())?;
            audit::log_db_actor(db, user.id, "invites", self.id, "fulfil", ());

            diesel::delete(&self.data).execute(db)?;

            Ok(user)
        })
    }

    pub fn send_mail(&self, locale: &'static Locale) -> SendFuture {
        let (url, locale, mail, subject) = self.prepare_mail(locale);
        Mailer::send(
            self.email.as_str(),
            mail,
            subject,
            InviteMailArgs {
                url: &url,
                email: &self.email,
                team: &self.team.name,
            },
            locale,
        )
    }

    pub fn do_send_mail(&self, locale: &'static Locale) {
        let (url, locale, mail, subject) = self.prepare_mail(locale);
        Mailer::do_send(
            self.email.as_str(),
            mail,
            subject,
            InviteMailArgs {
                url: &url,
                email: &self.email,
                team: &self.team.name,
            },
            locale,
        );
    }

    fn prepare_mail(&self, locale: &'static Locale)
    -> (String, &'static Locale, &'static str, &'static str) {
        let (url, locale, mail, subject) = match self.user {
            Some(ref user) => (
                "join/team",
                adaptarr_i18n::load()
                    .expect("localization should be initialized at this point")
                    .find_locale(&user.language())
                    .unwrap_or(locale),
                "team-invite",
                "mail-team-invite-subject",
            ),
            None => ("register", locale, "invite", "mail-invite-subject"),
        };

        let code = self.get_code(Config::secret());
        let url = format!("https://{}/{}?invite={}", Config::domain(), url, code);

        (url, locale, mail, subject)
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

#[derive(ApiError, Debug, Fail, From)]
pub enum FulfilInviteError {
    #[api(internal)]
    #[fail(display = "{}", _0)]
    Database(#[cause] #[from] DbError),
    #[fail(display = "{}", _0)]
    CreateUser(#[cause] #[from] CreateUserError),
    #[fail(display = "{}", _0)]
    AddMember(#[cause] #[from] AddMemberError),
    #[api(internal)]
    #[fail(
        display = "tried to use invitation for a new user was used as an \
            invitation for an existing user",
    )]
    NoUser,
}

/// Arguments for `mail/invite`.
#[derive(Serialize)]
struct InviteMailArgs<'a> {
    /// Registration URL.
    url: &'a str,
    /// Email address which was invited.
    email: &'a str,
    /// Name of the team the user is invited to.
    team: &'a str,
}

#[derive(Serialize)]
struct LogNewInvite<'a> {
   email: &'a str,
   expires: DateTime<Utc>,
   role: Option<i32>,
}

#[derive(Serialize)]
struct LogNewTeamInvite {
   expires: DateTime<Utc>,
   role: Option<i32>,
   user: i32,
}
