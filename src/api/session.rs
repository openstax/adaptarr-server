//! Session management.

use actix_web::{
    HttpRequest,
    HttpResponse,
    FromRequest,
    middleware::{Middleware, Started, Response},
    error::{ErrorInternalServerError, Result},
    http::Cookie,
};
use chrono::{Duration, Utc};
use cookie::SameSite;
use diesel::{prelude::*, result::{Error as DbError}};
use failure::Fail;
use std::marker::PhantomData;

use crate::{
    ApiError,
    audit,
    config,
    db::{
        self,
        Connection,
        Pool,
        models::{Session as DbSession, NewSession, SessionUpdate},
        schema::sessions,
    },
    models::user::{User, FindUserError},
    permissions::{Permission, PermissionBits, RequirePermissionsError},
    utils,
};
use super::{Error, State};

/// Name of the cookie carrying session ID.
const COOKIE: &str = "sesid";

/// Maximal age of a session, after which user will be required to
/// re-authenticate. Defaults to 30 days.
const MAX_DURATION: i64 = 30;

/// Time which must pass for session to be considered expired due to inactivity,
/// defaults to seven days.
const INACTIVITY_EXPIRATION: i64 = 7;

/// Time after which an administrative session will be downgraded back to
/// a normal session. Defaults to 15 minutes.
const SUPER_EXPIRATION: i64 = 15;

pub struct SessionManager {
    /// Secret key used to seal and unseal session cookies.
    secret: Vec<u8>,
    /// Domain for which session cookies are valid.
    domain: &'static str,
    /// Pool of database connections.
    db: Pool,
}

/// Session extractor.
///
/// Extract session data from request or reject it. Requests can be rejected
/// when session cookie (sesid) is missing (401), when it was corrupted (400),
/// or by the [`Policy`] chosen.
pub struct Session<Policy = Normal> {
    data: DbSession,
    _policy: PhantomData<Policy>,
}

/// Policies govern what sessions can do. For example a [`Normal`] session can
/// not be used to modify server settings, only an [`Elevated`] session can
/// do so.
///
/// When implementing a policy you can assume the session itself is valid,
/// as policies are only checked after a session was validated. To put
/// it differently, you can assume that the session you are validating has
/// already passed [`Normal`].
pub trait Policy {
    type Error;

    /// Validate a session.
    ///
    /// This method should return `true` if the request should pass, and `false`
    /// otherwise.
    fn validate(session: &DbSession) -> Validation<Self::Error>;
}

/// Outcome of policy validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Validation<E = Error> {
    /// Let this session through.
    Pass,
    /// Update this session.
    ///
    /// If the second argument is true then the session should be let through,
    /// otherwise it should be rejected. It will be updated regardless.
    Update(SessionUpdate, bool),
    /// Reject this session.
    Reject,
    /// Reject this session with a specific error.
    Error(E),
}

/// Normal policy.
///
/// This policy allows all sessions to pass.
///
/// This is the default policy.
pub struct Normal;

/// Data internal to the session manager.
struct SessionData {
    /// Existing session, if any.
    existing: Option<DbSession>,
    /// Data for a new session to be created.
    new: Option<NewSession>,
    /// Whether to destroy the existing session or not. Existing session
    /// is always destroyed if it is to be replaced with a new one.
    destroy: bool,
}

impl SessionManager {
    fn validate(ses: &DbSession) -> Validation {
        let now = Utc::now().naive_utc();

        // Disallow expired sessions.
        if now > ses.expires {
            return Validation::Reject;
        }

        let diff = now - ses.last_used;

        // Disallow reviving inactive sessions.
        if diff > Duration::days(INACTIVITY_EXPIRATION) {
            return Validation::Reject;
        }

        // Downgrade elevated session back to a normal session after some
        // time.
        if ses.is_elevated && diff > Duration::minutes(SUPER_EXPIRATION) {
            let permissions = ses.permissions & PermissionBits::normal().bits();

            return Validation::Update(SessionUpdate {
                is_elevated: Some(false),
                permissions: Some(permissions),
                .. SessionUpdate::default()
            }, true);
        }

        Validation::Pass
    }
}

impl Default for SessionManager {
    fn default() -> SessionManager {
        let config = config::load().expect("configuration should be ready");
        let db = db::pool().expect("database should be ready");

        SessionManager {
            secret: config.server.secret.clone(),
            domain: &config.server.domain,
            db,
        }
    }
}

impl<S> Middleware<S> for SessionManager {
    fn start(&self, req: &HttpRequest<S>) -> Result<Started> {
        let cookie = match req.cookie(COOKIE) {
            Some(cookie) => cookie,
            None => return Ok(Started::Done),
        };

        let mut data = match base64::decode(cookie.value()) {
            Ok(data) => data,
            Err(_) => return Ok(Started::Done),
        };
        let sesid: i32 = match utils::unseal(&self.secret, &mut data) {
            Ok(sesid) => sesid,
            Err(_) => return Ok(Started::Done),
        };

        let db = self.db.get()
            .map_err(|e| ErrorInternalServerError(e.to_string()))?;

        let session = sessions::table
            .filter(sessions::id.eq(sesid))
            .get_result::<DbSession>(&*db)
            .optional()
            .map_err(|e| ErrorInternalServerError(e.to_string()))?;

        let session = match session {
            Some(session) => session,
            None => return Ok(Started::Done),
        };

        let pass = match SessionManager::validate(&session) {
            Validation::Pass => Some(session),
            Validation::Update(update, _) => {
                diesel::update(&session)
                    .set(update)
                    .get_result::<DbSession>(&*db)
                    .map(Some)
                    .map_err(|e| ErrorInternalServerError(e.to_string()))?
            }
            Validation::Reject | Validation::Error(_) => {
                diesel::delete(&session)
                    .execute(&*db)
                    .map_err(|e| ErrorInternalServerError(e.to_string()))?;
                None
            }
        };

        if let Some(session) = pass {
            req.extensions_mut().insert(SessionData {
                existing: Some(session),
                new: None,
                destroy: false,
            });
            audit::set_actor(audit::Actor::User(session.user));
        }

        Ok(Started::Done)
    }

    fn response(&self, req: &HttpRequest<S>, mut rsp: HttpResponse) -> Result<Response> {
        if let Some(session) = req.extensions().get::<SessionData>() {
            audit::set_actor(None);

            let now = Utc::now().naive_utc();
            let db = self.db.get()
                .map_err(|e| ErrorInternalServerError(e.to_string()))?;

            if session.existing.is_some() && session.destroy {
                diesel::delete(&session.existing.unwrap())
                    .execute(&*db)
                    .map_err(|e| ErrorInternalServerError(e.to_string()))?;
                let cookie = Cookie::build(COOKIE, "")
                    .domain(self.domain)
                    .path("/")
                    .max_age(Duration::zero())
                    .secure(!cfg!(debug_assertions))
                    .http_only(!cfg!(debug_assertions))
                    .same_site(SameSite::Strict)
                    .finish();
                rsp.add_cookie(&cookie)?;
            } else if let Some(new) = session.new {
                if let Some(session) = session.existing {
                    diesel::delete(&session)
                        .execute(&*db)
                        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
                }

                let session = diesel::insert_into(sessions::table)
                    .values(new)
                    .get_result::<DbSession>(&*db)
                    .map_err(|e| ErrorInternalServerError(e.to_string()))?;

                let value = utils::seal(&self.secret, session.id)
                    .expect("sealing session ID");
                let cookie = Cookie::build(COOKIE, base64::encode(&value))
                    .domain(self.domain)
                    .path("/")
                    .max_age(Duration::days(MAX_DURATION))
                    .secure(!cfg!(debug_assertions))
                    .http_only(!cfg!(debug_assertions))
                    .same_site(SameSite::Strict)
                    .finish();
                rsp.add_cookie(&cookie)?;
            } else if let Some(session) = session.existing {
                diesel::update(&session)
                    .set(sessions::last_used.eq(now))
                    .execute(&*db)
                    .map_err(|e| ErrorInternalServerError(e.to_string()))?;
            }
        }

        Ok(Response::Done(rsp))
    }
}

impl<P> Session<P> {
    pub fn create<S>(req: &HttpRequest<S>, user: &User, is_elevated: bool) {
        let mask = if is_elevated {
            PermissionBits::elevated()
        } else {
            PermissionBits::normal()
        };
        let permissions = if user.is_super {
            PermissionBits::all()
        } else {
            user.permissions(true)
        };

        let now = Utc::now().naive_utc();
        let new = NewSession {
            user: user.id,
            is_elevated,
            expires: now + Duration::days(MAX_DURATION),
            last_used: now,
            permissions: (permissions & mask).bits(),
        };

        let mut extensions = req.extensions_mut();

        if let Some(session) = extensions.get_mut::<SessionData>() {
            session.new = Some(new);
            return;
        }

        extensions.insert(SessionData {
            existing: None,
            new: Some(new),
            destroy: false,
        });
    }

    pub fn destroy<S>(req: &HttpRequest<S>, sess: Self) {
        req.extensions_mut().insert(SessionData {
            existing: Some(sess.data),
            new: None,
            destroy: true,
        })
    }

    pub fn user_id(&self) -> i32 {
        self.data.user
    }

    pub fn user(&self, dbcon: &Connection) -> Result<User, DbError> {
        match User::by_id(dbcon, self.data.user) {
            Ok(user) => Ok(user),
            Err(FindUserError::Internal(err)) => Err(err),
            Err(err) => {
                panic!("Inconsistency: session's user doesn't exist: {}", err);
            }
        }
    }

    pub fn permissions(&self) -> PermissionBits {
        PermissionBits::from_bits_truncate(self.data.permissions)
    }
}

impl<P> std::ops::Deref for Session<P> {
    type Target = DbSession;

    fn deref(&self) -> &DbSession {
        &self.data
    }
}


impl<P> FromRequest<State> for Session<P>
where
    P: Policy,
    Error: From<P::Error>,
{
    type Config = ();
    type Result = Result<Session<P>, Error>;

    fn from_request(req: &HttpRequest<State>, _cfg: &()) -> Self::Result {
        if let Some(session) = req.extensions().get::<SessionData>()
                                .and_then(|s| s.existing) {

            match P::validate(&session) {
                Validation::Pass => (),
                Validation::Update(update, pass) => {
                    let db = req.state().db.get()?;
                    diesel::update(&session)
                        .set(update)
                        .execute(&*db)?;

                    if !pass {
                        return Err(SessionFromRequestError::Policy.into());
                    }
                }
                Validation::Reject =>
                    return Err(SessionFromRequestError::Policy.into()),
                Validation::Error(error) => return Err(error.into()),
            }

            Ok(Session {
                data: session,
                _policy: PhantomData,
            })
        } else {
            Err(SessionFromRequestError::NoSession.into())
        }
    }
}

impl Policy for Normal {
    type Error = Error;

    fn validate(_: &DbSession) -> Validation {
        Validation::Pass
    }
}

impl<P: Permission> Policy for P {
    type Error = RequirePermissionsError;

    fn validate(session: &DbSession) -> Validation<RequirePermissionsError> {
        let bits = PermissionBits::from_bits_truncate(session.permissions);

        match bits.require(P::bits()) {
            Ok(()) => Validation::Pass,
            Err(err) => Validation::Error(err),
        }
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum SessionFromRequestError {
    #[api(status = "UNAUTHORIZED", code = "user:session:required")]
    #[fail(display = "A session is required to access this resource")]
    NoSession,
    #[api(internal)]
    #[fail(display = "Unsealing error: {}", _0)]
    Unsealing(#[cause] utils::UnsealingError),
    #[api(internal)]
    #[fail(display = "Invalid base64: {}", _0)]
    Decoding(#[cause] base64::DecodeError),
    /// Session was rejected by policy.
    #[api(status = "FORBIDDEN", code = "user:session:rejected")]
    #[fail(display = "Rejected by policy")]
    Policy,
}
