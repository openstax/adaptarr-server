use bitflags::bitflags;
use diesel::{
    Connection as _Connection,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use failure::Fail;
use lettre_email::Mailbox;
use rand::RngCore;
use serde::Serialize;

use crate::{
    ApiError,
    audit,
    db::{
        Connection,
        models as db,
        schema::{invites, users, password_reset_tokens, roles, sessions},
    },
    i18n::LanguageTag,
    permissions::PermissionBits,
};
use super::role::{Role, PublicData as RoleData};

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
    role: Option<Role>,
}

/// A subset of user's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    id: i32,
    name: String,
    is_super: bool,
    language: String,
    #[serde(skip_serializing_if="Option::is_none")]
    permissions: Option<PermissionBits>,
    role: Option<RoleData>,
}

bitflags! {
    /// Flags controlling which fields are included in [`User::get_public()`].
    pub struct Fields: u32 {
        /// Include user's permissions ([`PublicData::permissions`]).
        const PERMISSIONS = 0x0000_0001;
        /// Include user's role's permissions.
        const ROLE_PERMISSIONS = 0x0000_0002;
    }
}

impl User {
    /// Get all users.
    pub fn all(dbcon: &Connection) -> Result<Vec<User>, DbError> {
        users::table
            .left_join(roles::table)
            .get_results::<(db::User, Option<db::Role>)>(dbcon)
            .map(|v| {
                v.into_iter()
                    .map(|(data, role)| User {
                        data,
                        role: role.map(Role::from_db),
                    })
                    .collect()
            })
    }

    /// Find an user by ID.
    pub fn by_id(dbcon: &Connection, id: i32) -> Result<User, FindUserError> {
        users::table
            .filter(users::id.eq(id))
            .left_join(roles::table)
            .get_result::<(db::User, Option<db::Role>)>(dbcon)
            .optional()?
            .ok_or(FindUserError::NotFound)
            .map(|(data, role)| User {
                data,
                role: role.map(Role::from_db),
            })
    }

    /// Find an user by email address.
    pub fn by_email(dbcon: &Connection, email: &str) -> Result<User, FindUserError> {
        users::table
            .filter(users::email.eq(email))
            .left_join(roles::table)
            .get_result::<(db::User, Option<db::Role>)>(dbcon)
            .optional()?
            .ok_or(FindUserError::NotFound)
            .map(|(data, role)| User {
                data,
                role: role.map(Role::from_db),
            })
    }

    /// Create a new user.
    pub fn create(
        dbcon: &Connection,
        actor: Option<i32>,
        email: &str,
        name: &str,
        password: &str,
        is_super: bool,
        language: &str,
        permissions: PermissionBits,
        role: Option<&Role>,
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

        dbcon.transaction(|| {
            diesel::delete(invites::table.filter(invites::email.eq(email)))
                .execute(dbcon)
                .map_err(CreateUserError::Internal)?;

            let data = diesel::insert_into(users::table)
                .values(db::NewUser {
                    email,
                    name,
                    password: &hash,
                    salt: &salt,
                    is_super,
                    language,
                    permissions: if is_super {
                        std::i32::MAX
                    } else {
                        permissions.bits()
                    },
                    role: role.map(|r| r.id),
                })
                .get_result::<db::User>(dbcon)?;

            let actor = actor.unwrap_or(data.id);
            audit::log_db_actor(
                dbcon, actor, "users", data.id, "create", LogNewUser {
                    email,
                    name,
                    is_super,
                    language,
                    permissions: data.permissions,
                });

            Ok(User { data, role: None })
        })
    }

    /// Get underlying database models.
    pub fn into_db(self) -> (db::User, Option<db::Role>) {
        (self.data, self.role.map(Role::into_db))
    }

    /// Find an user for given email and try to authenticate as them.
    pub fn authenticate(dbcon: &Connection, email: &str, password: &str)
    -> Result<User, UserAuthenticateError> {
        let user = User::by_email(dbcon, email)?;

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

    /// Get the public portion of this user's data.
    pub fn get_public(&self, fields: Fields) -> PublicData {
        let db::User { id, ref name, is_super, ref language, .. } = self.data;

        let permissions = if fields.contains(Fields::PERMISSIONS) {
            Some(PermissionBits::from_bits_truncate(self.data.permissions))
        } else {
            None
        };

        PublicData {
            id,
            name: name.clone(),
            is_super,
            language: language.clone(),
            permissions,
            role: self.role.as_ref().map(|r|
                r.get_public(fields.contains(Fields::ROLE_PERMISSIONS))),
        }
    }

    pub fn language(&self) -> LanguageTag {
        self.data.language.parse().expect("invalid language tag in database")
    }

    /// Get all permissions this user has.
    ///
    /// The `role` argument controls whether role permissions are included in
    /// the returned permission set.
    pub fn permissions(&self, role: bool) -> PermissionBits {
        let role = if role {
            self.role.as_ref().map(Role::permissions).unwrap_or_default()
        } else {
            PermissionBits::empty()
        };
        PermissionBits::from_bits_truncate(self.data.permissions) | role
    }

    pub fn mailbox(&self) -> Mailbox {
        Mailbox::new(self.data.email.clone())
    }

    /// Change user's password.
    pub fn change_password(&mut self, dbcon: &Connection, password: &str)
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

        let data = dbcon.transaction(|| {
            // Delete all existing password reset tokens.
            diesel::delete(
                password_reset_tokens::table
                    .filter(password_reset_tokens::user.eq(self.id)))
                .execute(dbcon)?;

            // Delete all existing sessions.
            diesel::delete(sessions::table.filter(sessions::user.eq(self.id)))
                .execute(dbcon)?;

            audit::log_db(dbcon, "users", self.id, "change-password", ());

            // Update credentials.
            diesel::update(&self.data)
                .set(db::PasswordChange {
                    salt: &salt,
                    password: &hash,
                })
                .get_result::<db::User>(dbcon)
        })?;

        self.data = data;

        Ok(())
    }

    /// Change user's name.
    pub fn set_name(&mut self, dbcon: &Connection, name: &str)
    -> Result<(), DbError> {
        self.data = diesel::update(&self.data)
            .set(users::name.eq(name))
            .get_result::<db::User>(dbcon)?;

        audit::log_db(dbcon, "users", self.id, "set-name", name);

        Ok(())
    }

    /// Change user's preferred language.
    pub fn set_language(
        &mut self,
        dbcon: &Connection,
        language: &LanguageTag,
    ) -> Result<(), DbError> {
        let data = diesel::update(&self.data)
            .set(users::language.eq(language.as_str()))
            .get_result::<db::User>(dbcon)?;

        self.data = data;

        audit::log_db(dbcon, "users", self.id, "change-language", language);

        Ok(())
    }

    /// Change user's permissions.
    pub fn set_permissions(
        &mut self,
        dbcon: &Connection,
        permissions: PermissionBits,
    ) -> Result<(), DbError> {
        // Superusers have all permissions.
        if self.data.is_super {
            return Ok(());
        }

        let sessions_perms = permissions
            | self.role.as_ref().map_or(PermissionBits::empty(), Role::permissions);

        let data = dbcon.transaction(|| {
            audit::log_db(
                dbcon, "users", self.id, "set-permissions", permissions.bits());

            // Since we might be removing a permission we also need to update
            // user's sessions.
            diesel::update(sessions::table.filter(
                    sessions::user.eq(self.id).and(
                        sessions::is_elevated.eq(false))))
                .set(sessions::permissions.eq(
                    (sessions_perms & PermissionBits::normal()).bits()))
                .execute(dbcon)?;
            diesel::update(sessions::table.filter(
                    sessions::user.eq(self.id).and(
                        sessions::is_elevated.eq(false))))
                .set(sessions::permissions.eq(sessions_perms.bits()))
                .execute(dbcon)?;

            diesel::update(&self.data)
                .set(users::permissions.eq(permissions.bits()))
                .get_result::<db::User>(dbcon)
        })?;

        self.data = data;

        Ok(())
    }

    /// Change user's role.
    pub fn set_role(
        &mut self,
        dbcon: &Connection,
        role: Option<&Role>,
    ) -> Result<(), DbError> {
        let (role_id, sessions_perms) = match role {
            Some(role) => (
                Some(role.id),
                self.permissions(false) | role.permissions(),
            ),
            None => (None, self.permissions(false)),
        };

        let data = dbcon.transaction(|| {
            audit::log_db(dbcon, "users", self.id, "set-role", role_id);

            // Since user's previous role might have had more permissions
            // we also need to update user's sessions.
            diesel::update(sessions::table.filter(
                    sessions::user.eq(self.id).and(
                        sessions::is_elevated.eq(false))))
                .set(sessions::permissions.eq(
                    (sessions_perms & PermissionBits::normal()).bits()))
                .execute(dbcon)?;
            diesel::update(sessions::table.filter(
                    sessions::user.eq(self.id).and(
                        sessions::is_elevated.eq(true))))
                .set(sessions::permissions.eq(sessions_perms.bits()))
                .execute(dbcon)?;

            diesel::update(&self.data)
                .set(users::role.eq(role_id))
                .get_result::<db::User>(dbcon)
        })?;

        self.data = data;
        self.role = role.map(Clone::clone);

        Ok(())
    }
}

impl std::ops::Deref for User {
    type Target = db::User;

    fn deref(&self) -> &db::User {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindUserError {
    /// Creation failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Internal(#[cause] DbError),
    /// No user found for given email address.
    #[fail(display = "No such user")]
    #[api(code = "user:not-found", status = "NOT_FOUND")]
    NotFound,
}

impl_from! { for FindUserError ;
    DbError => |e| FindUserError::Internal(e),
}

#[derive(ApiError, Debug, Fail)]
pub enum CreateUserError {
    /// Creation failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Internal(#[cause] DbError),
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

impl_from! { for CreateUserError ;
    DbError => |e| match e {
        DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)
            => CreateUserError::Duplicate,
        _ => CreateUserError::Internal(e),
    },
}

#[derive(ApiError, Debug, Fail)]
pub enum UserAuthenticateError {
    /// Authentication failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Internal(#[cause] DbError),
    /// No user found for given email address.
    #[fail(display = "No such user")]
    #[api(code = "user:not-found", status = "NOT_FOUND")]
    NotFound,
    /// Provided password was not valid for the user.
    #[fail(display = "Bad password")]
    #[api(code = "user:authenticate:bad-password", status = "FORBIDDEN")]
    BadPassword,
}

impl_from! { for UserAuthenticateError ;
    DbError => |e| UserAuthenticateError::Internal(e),
    FindUserError => |e| match e {
        FindUserError::Internal(e) => UserAuthenticateError::Internal(e),
        FindUserError::NotFound => UserAuthenticateError::NotFound,
    },
}

#[derive(ApiError, Debug, Fail)]
pub enum ChangePasswordError {
    /// Authentication failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Internal(#[cause] DbError),
    #[fail(display = "Password cannot be empty")]
    #[api(code = "user:change-password:empty", status = "BAD_REQUEST")]
    EmptyPassword,
}

impl_from! { for ChangePasswordError ;
    DbError => |e| ChangePasswordError::Internal(e),
}

#[derive(Serialize)]
struct LogNewUser<'a> {
    email: &'a str,
    name: &'a str,
    is_super: bool,
    language: &'a str,
    // XXX: we serialize permissions as bits as rmp-serde currently works as
    // a human-readable format, and serializes PermissionBits as an array of
    // strings.
    permissions: i32,
}
