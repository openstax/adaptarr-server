use adaptarr_error::ApiError;
use adaptarr_i18n::{LanguageTag, Locale};
use adaptarr_macros::From;
use adaptarr_mail::{Mailbox, Mailer, IntoSubject};
use diesel::{
    Connection as _,
    expression::dsl::any,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use failure::Fail;
use rand::RngCore;
use serde::Serialize;

use crate::{
    audit,
    db::{
        Connection,
        models as db,
        schema::{
            invites,
            password_reset_tokens,
            roles,
            sessions,
            team_members,
            teams,
            users,
        },
    },
    permissions::TeamPermissions,
};
use super::{FindModelError, FindModelResult, Model, Role, Team};

static ARGON2_CONFIG: argon2::Config = argon2::Config {
    ad: &[],
    hash_length: 32,
    lanes: 1,
    mem_cost: 4096,
    secret: &[],
    thread_mode: argon2::ThreadMode::Sequential,
    time_cost: 3,
    variant: argon2::Variant::Argon2id,
    version: argon2::Version::Version13,
};

/// A single user in the system.
#[derive(Debug)]
pub struct User {
    data: db::User,
}

/// A subset of user's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    id: i32,
    name: String,
    is_super: bool,
    language: String,
    #[serde(skip_serializing_if="Option::is_none")]
    teams: Option<Vec<TeamInfo>>,
}

#[derive(Debug, Serialize)]
pub struct TeamInfo {
    #[serde(rename = "id")]
    team: i32,
    role: Option<<Role as Model>::Public>,
    name: String,
    permissions: TeamPermissions,
}

#[derive(Default)]
pub struct PublicParams {
    /// Include user's system permissions ([`PublicData::permissions`]) and team
    /// permissions.
    pub include_permissions: bool,
    /// Teams to include in [`Public::teams`].
    ///
    /// Specifying `None` is equivalent to a vector with all team included.
    // FIXME: This can't be a slice until generic associated types are stable.
    pub include_teams: Option<Vec<i32>>,
}

impl Model for User {
    const ERROR_CATEGORY: &'static str = "user";

    type Id = i32;
    type Database = db::User;
    type Public = Public;
    type PublicParams = PublicParams;

    fn by_id(db: &Connection, id: Self::Id)
    -> FindModelResult<Self> {
        users::table
            .filter(users::id.eq(id))
            .get_result(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        User { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Public {
        let db::User { id, ref name, is_super, ref language, .. } = self.data;

        Public {
            id,
            name: name.clone(),
            is_super,
            language: language.clone(),
            teams: None,
        }
    }

    fn get_public_full(&self, db: &Connection, params: &PublicParams)
    -> Result<Public, DbError> {
        let db::User { id, ref name, is_super, ref language, .. } = self.data;

        let teams = match params.include_teams {
            Some(ref teams) => team_members::table
                .filter(team_members::team.eq(any(&teams))
                    .and(team_members::user.eq(self.data.id)))
                .left_join(roles::table)
                .inner_join(teams::table)
                .get_results::<(db::TeamMember, Option<db::Role>, db::Team)>(db)?,
            None => team_members::table
                .filter(team_members::user.eq(self.data.id))
                .left_join(roles::table)
                .inner_join(teams::table)
                .get_results::<(db::TeamMember, Option<db::Role>, db::Team)>(db)?,
        }
            .into_iter()
            .map(|(member, role, team)| Ok(TeamInfo {
                team: member.team,
                role: role.map(|id| Role::from_db(id)
                    .get_public_full(db, &params.include_permissions))
                    .transpose()?,
                name: team.name,
                permissions: TeamPermissions::from_bits_truncate(member.permissions),
            }))
            .collect::<Result<Vec<TeamInfo>, DbError>>()?;

        Ok(Public {
            id,
            name: name.clone(),
            is_super,
            language: language.clone(),
            teams: Some(teams),
        })
    }
}

impl User {
    /// Get all users.
    pub fn all(db: &Connection) -> Result<Vec<User>, DbError> {
        users::table
            .get_results::<db::User>(db)
            .map(|v| v.into_iter().map(Self::from_db).collect())
    }

    /// Get all users in specified teams.
    pub fn by_team(db: &Connection, teams: &[i32])
    -> Result<Vec<User>, DbError> {
        users::table
            .inner_join(team_members::table)
            .filter(team_members::team.eq(any(teams)))
            .select(users::all_columns)
            .get_results(db)
            .map(Model::from_db)
    }

    /// Find an user by email address.
    pub fn by_email(db: &Connection, email: &str)
    -> FindModelResult<User> {
        users::table
            .filter(users::email.eq(email))
            .get_result(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    /// Create a new user.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        db: &Connection,
        actor: Option<i32>,
        email: &str,
        name: &str,
        password: &str,
        is_super: bool,
        language: &str,
    ) -> Result<User, CreateUserError> {
        if name.is_empty() {
            return Err(CreateUserError::EmptyName);
        }

        if password.is_empty() {
            return Err(CreateUserError::EmptyPassword);
        }

        // Generate salt and hash password.
        let mut salt = [0; 16];
        rand::thread_rng().fill_bytes(&mut salt);

        // Hashing can only fail if the configuration is invalid, or salt
        // is wrong length. All those cases are unlikely.
        let hash = argon2::hash_raw(
            password.as_bytes(),
            &salt,
            &ARGON2_CONFIG,
        ).expect("Cannot hash password");

        db.transaction(|| {
            diesel::delete(invites::table.filter(invites::email.eq(email)))
                .execute(db)
                .map_err(CreateUserError::Database)?;

            let data = diesel::insert_into(users::table)
                .values(db::NewUser {
                    email,
                    name,
                    password: &hash,
                    salt: &salt,
                    is_super,
                    language,
                })
                .get_result::<db::User>(db)?;

            let actor = actor.unwrap_or(data.id);
            audit::log_db_actor(
                db, actor, "users", data.id, "create", LogNewUser {
                    email,
                    name,
                    is_super,
                    language,
                });

            Ok(User { data })
        })
    }

    /// Find an user for given email and try to authenticate as them.
    pub fn authenticate(db: &Connection, email: &str, password: &str)
    -> Result<User, UserAuthenticateError> {
        let user = User::by_email(db, email)?;

        if user.check_password(password) {
            Ok(user)
        } else {
            Err(UserAuthenticateError::BadPassword)
        }
    }

    /// Verify correctness of a password.
    pub fn check_password(&self, password: &str) -> bool {
        // Verification can only fail if the configuration is invalid, or salt
        // or password digest length are wrong. All those cases are unlikely.
        argon2::verify_raw(
            password.as_bytes(),
            &self.data.salt,
            &self.data.password,
            &ARGON2_CONFIG,
        ).expect("hashing password")
    }

    /// Get user's preferred language.
    pub fn language(&self) -> LanguageTag {
        self.data.language.parse().expect("invalid language tag in database")
    }

    /// Get [`Locale`] for user's preferred language.
    pub fn locale(&self) -> &'static Locale {
        adaptarr_i18n::load()
            .expect("locale data should be loaded at this point")
            .find_locale(&self.language())
            .expect("locale data missing for user's language")
    }

    pub fn mailbox(&self) -> Mailbox {
        Mailbox::new(self.data.email.clone())
    }

    /// Get list of IDs of all teams this user is a member of.
    pub fn get_team_ids(&self, db: &Connection)
    -> Result<Vec<<Team as Model>::Id>, DbError> {
        team_members::table
            .filter(team_members::user.eq(self.data.id))
            .select(team_members::team)
            .get_results(db)
    }

    /// Get list of all teams this user is a member of.
    pub fn get_teams(&self, db: &Connection)
    -> Result<Vec<Team>, DbError> {
        team_members::table
            .filter(team_members::user.eq(self.data.id))
            .inner_join(teams::table)
            .select(teams::all_columns)
            .get_results(db)
            .map(Model::from_db)
    }

    /// Change user's password.
    pub fn change_password(&mut self, db: &Connection, password: &str)
    -> Result<(), ChangePasswordError> {
        if password.is_empty() {
            return Err(ChangePasswordError::EmptyPassword);
        }

        // Generate salt and hash password.
        let mut salt = [0; 16];
        rand::thread_rng().fill_bytes(&mut salt);

        // Hashing can only fail if the configuration is invalid, or salt
        // is wrong length. All those cases are unlikely.
        let hash = argon2::hash_raw(
            password.as_bytes(),
            &salt,
            &ARGON2_CONFIG,
        ).expect("Cannot hash password");

        let data = db.transaction(|| {
            // Delete all existing password reset tokens.
            diesel::delete(
                password_reset_tokens::table
                    .filter(password_reset_tokens::user.eq(self.id)))
                .execute(db)?;

            // Delete all existing sessions.
            diesel::delete(sessions::table.filter(sessions::user.eq(self.id)))
                .execute(db)?;

            audit::log_db(db, "users", self.id, "change-password", ());

            // Update credentials.
            diesel::update(&self.data)
                .set(db::PasswordChange {
                    salt: &salt,
                    password: &hash,
                })
                .get_result::<db::User>(db)
        })?;

        self.data = data;

        Ok(())
    }

    /// Change user's name.
    pub fn set_name(&mut self, db: &Connection, name: &str)
    -> Result<(), DbError> {
        self.data = diesel::update(&self.data)
            .set(users::name.eq(name))
            .get_result::<db::User>(db)?;

        audit::log_db(db, "users", self.id, "set-name", name);

        Ok(())
    }

    /// Change user's preferred language.
    pub fn set_language(
        &mut self,
        db: &Connection,
        language: &LanguageTag,
    ) -> Result<(), DbError> {
        let data = diesel::update(&self.data)
            .set(users::language.eq(language.as_str()))
            .get_result::<db::User>(db)?;

        self.data = data;

        audit::log_db(db, "users", self.id, "change-language", language);

        Ok(())
    }

    pub fn do_send_mail<S, C>(
        &self,
        template: &str,
        subject: S,
        context: C,
    )
    where
        S: IntoSubject,
        C: Serialize,
    {
        Mailer::do_send(self.mailbox(), template, subject, context, self.locale());
    }
}

impl std::ops::Deref for User {
    type Target = db::User;

    fn deref(&self) -> &db::User {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum CreateUserError {
    /// Creation failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// Duplicate user.
    #[fail(display = "Duplicate user")]
    #[api(code = "user:new:exists", status = "BAD_REQUEST")]
    Duplicate,
    #[fail(display = "User's name cannot be empty")]
    #[api(code = "user:new:empty-name", status = "BAD_REQUEST")]
    EmptyName,
    #[fail(display = "User's password cannot be empty")]
    #[api(code = "user:new:empty-password", status = "BAD_REQUEST")]
    EmptyPassword,
}

impl From<DbError> for CreateUserError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)
                => CreateUserError::Duplicate,
            _ => CreateUserError::Database(e),
        }
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum UserAuthenticateError {
    /// Authentication failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// No user found for given email address.
    #[fail(display = "No such user")]
    #[api(code = "user:not-found", status = "NOT_FOUND")]
    NotFound,
    /// Provided password was not valid for the user.
    #[fail(display = "Bad password")]
    #[api(code = "user:authenticate:bad-password", status = "FORBIDDEN")]
    BadPassword,
}

impl From<FindModelError<User>> for UserAuthenticateError {
    fn from(e: FindModelError<User>) -> Self {
        match e {
            FindModelError::Database(_, e) => UserAuthenticateError::Database(e),
            FindModelError::NotFound(_) => UserAuthenticateError::NotFound,
        }
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum ChangePasswordError {
    /// Authentication failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Internal(#[cause] #[from] DbError),
    #[fail(display = "Password cannot be empty")]
    #[api(code = "user:change-password:empty", status = "BAD_REQUEST")]
    EmptyPassword,
}

#[derive(Serialize)]
struct LogNewUser<'a> {
    email: &'a str,
    name: &'a str,
    is_super: bool,
    language: &'a str,
}
