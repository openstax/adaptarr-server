use diesel::{
    Connection as _Connection,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use rand::RngCore;

use crate::{
    db::{
        Connection,
        models as db,
        schema::{invites, users, password_reset_tokens, sessions},
    },
};

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
pub struct PublicData {
    id: i32,
    name: String,
    is_super: bool,
    language: String,
}

impl User {
    /// Get all users.
    pub fn all(dbcon: &Connection) -> Result<Vec<User>, DbError> {
        users::table
            .get_results::<db::User>(dbcon)
            .map(|v| v.into_iter().map(|data| User { data }).collect())
    }

    /// Find an user by ID.
    pub fn by_id(dbcon: &Connection, id: i32) -> Result<User, FindUserError> {
        users::table
            .filter(users::id.eq(id))
            .get_result::<db::User>(dbcon)
            .optional()?
            .ok_or(FindUserError::NotFound)
            .map(|data| User { data })
    }

    /// Find an user by email address.
    pub fn by_email(dbcon: &Connection, email: &str) -> Result<User, FindUserError> {
        users::table
            .filter(users::email.eq(email))
            .get_result::<db::User>(dbcon)
            .optional()?
            .ok_or(FindUserError::NotFound)
            .map(|data| User { data })
    }

    /// Create a new user.
    pub fn create(
        dbcon: &Connection,
        email: &str,
        name: &str,
        password: &str,
        is_super: bool,
        language: &str,
    ) -> Result<User, CreateUserError> {
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

            diesel::insert_into(users::table)
                .values(db::NewUser {
                    email,
                    name,
                    password: &hash,
                    salt: &salt,
                    is_super,
                    language,
                })
                .get_result::<db::User>(dbcon)
                .map(|data| User { data })
                .map_err(Into::into)
        })
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
    pub fn get_public(&self) -> PublicData {
        let db::User { id, ref name, is_super, ref language, .. } = self.data;

        PublicData {
            id,
            name: name.clone(),
            is_super,
            language: language.clone(),
        }
    }

    /// Change user's password.
    pub fn change_password(&mut self, dbcon: &Connection, password: &str)
    -> Result<(), ChangePasswordError> {
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
    #[api(code = "user:authenticate:bad-password", status = "BAD_REQUEST")]
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
}

impl_from! { for ChangePasswordError ;
    DbError => |e| ChangePasswordError::Internal(e),
}
