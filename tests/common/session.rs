//! Utilities for managing sessions in tests.

use actix_web::http::Cookie;
use adaptarr::{
    db::{models::{Session as DbSession, NewSession}, schema::sessions},
    models::User,
    permissions::PermissionBits,
};
use chrono::{Duration, NaiveDateTime, Utc};
use diesel::{PgConnection, prelude::*, result::Error as DbError};
use failure::Error;
use std::cell::RefCell;

use super::support::{ConfigureTest, Fixture, TestOptions};

/// Find an existing session by its ID.
pub fn find(dbcon: &PgConnection, id: i32) -> Result<Session, DbError> {
    sessions::table
        .filter(sessions::id.eq(id))
        .get_result::<DbSession>(dbcon)
        .map(Session::new)
}

#[derive(Clone)]
pub struct Session {
    data: DbSession,
    cookie: RefCell<Option<Cookie<'static>>>,
}

impl Session {
    fn new(data: DbSession) -> Self {
        Session {
            data,
            cookie: RefCell::new(None),
        }
    }

    /// Create a cookie for this session.
    pub fn cookie(&self) -> Cookie<'static> {
        self.cookie.borrow_mut().get_or_insert_with(|| {
            let value = adaptarr::utils::seal(&[0; 32], self.data.id).unwrap();
            Cookie::build("sesid", base64::encode(&value)).finish()
        }).clone()
    }

    /// Reload this session's data.
    pub fn reload(&mut self, dbcon: &PgConnection) -> Result<(), DbError> {
        self.data = sessions::table
            .filter(sessions::id.eq(self.data.id))
            .get_result::<DbSession>(dbcon)?;
        Ok(())
    }

    /// Get set of permissions this session has.
    pub fn permissions(&self) -> PermissionBits {
        PermissionBits::from_bits_truncate(self.data.permissions)
    }
}

impl std::ops::Deref for Session {
    type Target = DbSession;

    fn deref(&self) -> &DbSession {
        &self.data
    }
}

pub struct Builder<'db> {
    db: &'db PgConnection,
    user: i32,
    expires: NaiveDateTime,
    last_used: NaiveDateTime,
    is_elevated: bool,
    permissions: PermissionBits,
}

impl<'db> Builder<'db> {
    /// Build a new session.
    pub fn new(db: &'db PgConnection, user: i32) -> Self {
        let now = Utc::now().naive_utc();

        Builder {
            db,
            user,
            expires: now + Duration::days(30),
            last_used: now,
            is_elevated: false,
            permissions: PermissionBits::empty(),
        }
    }

    /// Set session's expiration date.
    pub fn expires(mut self, when: NaiveDateTime) -> Self {
        self.expires = when;
        self
    }

    /// Set time session was last used.
    pub fn last_used(mut self, when: NaiveDateTime) -> Self {
        self.last_used = when;
        self
    }

    /// Set whether this is an elevated session.
    pub fn elevated(mut self, elevated: bool) -> Self {
        self.is_elevated = elevated;
        self
    }

    /// Set session's permissions.
    pub fn permissions(mut self, bits: PermissionBits) -> Self {
        self.permissions.insert(bits);
        self
    }

    /// Build a new session.
    pub fn build(self) -> Result<Session, DbError> {
        let Builder {
            db, user, expires, last_used, is_elevated, permissions,
        } = self;

        diesel::insert_into(sessions::table)
            .values(NewSession {
                user, expires, last_used, is_elevated,
                permissions: permissions.bits(),
            })
            .get_result(db)
            .map(Session::new)
    }
}

impl Fixture for Session {
    fn make(opts: &TestOptions) -> Result<Self, Error> {
        opts.get()
            .map(Clone::clone)
            .ok_or_else(|| failure::format_err!(
                "No session configured, add #[adaptarr::test(session(options))] \
                to test"))
    }
}

/// Configure a session for test client.
///
/// ```
/// #[adaptarr::test(
///     session(
///         r#for = "email",
///         expires = expression,
///         last_used = expression,
///         is_elevated = boolean,
///         permissions = expression,
///     ),
/// )]
/// ```
///
/// all options except `r#for` are optional.
pub fn configure_session() -> SessionOptions {
    SessionOptions::default()
}

#[derive(Default)]
pub struct SessionOptions {
    email: Option<String>,
    expires: Option<NaiveDateTime>,
    last_used: Option<NaiveDateTime>,
    is_elevated: Option<bool>,
    permissions: Option<PermissionBits>,
}

impl SessionOptions {
    pub fn r#for<S>(mut self, email: S) -> Self
    where
        String: From<S>,
    {
        self.email = Some(email.into());
        self
    }

    pub fn expires<E>(mut self, when: E) -> Self
    where
        NaiveDateTime: From<E>,
    {
        self.expires = Some(when.into());
        self
    }

    pub fn last_used<E>(mut self, when: E) -> Self
    where
        NaiveDateTime: From<E>,
    {
        self.last_used = Some(when.into());
        self
    }

    pub fn elevated(mut self, elevated: bool) -> Self {
        self.is_elevated = Some(elevated);
        self
    }

    pub fn permissions<E>(mut self, bits: E) -> Self
    where
        PermissionBits: From<E>,
    {
        self.permissions = Some(bits.into());
        self
    }
}

impl ConfigureTest for SessionOptions {
    fn configure(self, opts: &mut TestOptions) -> Result<(), Error> {
        let email = self.email
            .ok_or_else(|| failure::format_err!(
                r#"No user for session, add #[adaptarr::test(session(r#for = \
                "email"))]"#))?;

        let db = opts.pool.get()?;
        let user = User::by_email(&*db, &email)?;

        let mut session = Builder::new(&*db, user.id);

        if let Some(expires) = self.expires {
            session = session.expires(expires);
        }

        if let Some(last_used) = self.last_used {
            session = session.last_used(last_used);
        }

        if let Some(is_elevated) = self.is_elevated {
            session = session
                .elevated(is_elevated)
                .permissions(user.permissions(true));
        }

        if let Some(permissions) = self.permissions {
            session = session.permissions(permissions);
        }

        opts.put(session.build()?);

        Ok(())
    }
}
