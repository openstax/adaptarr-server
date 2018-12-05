//! Session management.

use actix_web::{
    HttpRequest,
    HttpResponse,
    FromRequest,
    ResponseError,
    middleware::{Middleware, Started, Response},
    error::{ErrorInternalServerError, Result},
    http::Cookie,
};
use chrono::{Duration, Utc};
use diesel::prelude::*;
use std::marker::PhantomData;

use crate::{
    db::{
        Pool,
        models::{Session as DbSession, NewSession},
        schema::sessions,
    },
    utils,
};

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
    /// Validate a session.
    ///
    /// This method should return `true` if the request should pass, and `false`
    /// otherwise.
    fn validate(session: &DbSession) -> bool;
}

/// Normal policy.
///
/// This policy allows all sessions to pass.
///
/// This is the default policy.
pub struct Normal;

/// Elevated policy.
///
/// This policy only allows administrative sessions.
pub struct Elevated;

pub type ElevatedSession = Session<Elevated>;

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
    pub fn new(secret: Vec<u8>, db: Pool) -> SessionManager {
        SessionManager { secret, db }
    }

    fn validate(ses: &DbSession) -> bool {
        let now = Utc::now().naive_utc();

        // Disallow expired sessions.
        if now > ses.expires {
            return false;
        }

        let diff = now - ses.last_used;

        // Disallow reviving inactive sessions.
        if diff > Duration::days(INACTIVITY_EXPIRATION) {
            return false;
        }

        // Downgrade administrative session back to a normal session after some
        // time.
        if ses.is_super && diff > Duration::minutes(SUPER_EXPIRATION) {
            // TODO: downgrade session
            return false;
        }

        true
    }
}

impl<S> Middleware<S> for SessionManager {
    fn start(&self, req: &HttpRequest<S>) -> Result<Started> {
        let cookie = match req.cookie(COOKIE) {
            Some(cookie) => cookie,
            None => return Ok(Started::Done),
        };

        let mut data = base64::decode(cookie.value())
            .map_err(SessionFromRequestError::Decoding)?;
        let sesid: i32 = utils::unseal(&self.secret, &mut data)
            .map_err(SessionFromRequestError::Unsealing)?;

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

        if !SessionManager::validate(&session) {
            diesel::delete(&session)
                .execute(&*db)
                .map_err(|e| ErrorInternalServerError(e.to_string()))?;
        } else {
            req.extensions_mut().insert(SessionData {
                existing: Some(session),
                new: None,
                destroy: false,
            });
        }

        Ok(Started::Done)
    }

    fn response(&self, req: &HttpRequest<S>, mut rsp: HttpResponse) -> Result<Response> {
        if let Some(session) = req.extensions().get::<SessionData>() {
            let now = Utc::now().naive_utc();
            let db = self.db.get()
                .map_err(|e| ErrorInternalServerError(e.to_string()))?;

            if session.existing.is_some() && session.destroy {
                diesel::delete(&session.existing.unwrap())
                    .execute(&*db)
                    .map_err(|e| ErrorInternalServerError(e.to_string()))?;
                rsp.add_cookie(&Cookie::new(COOKIE, ""));
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
                    .path("/")
                    .max_age(Duration::days(MAX_DURATION))
                    .secure(true)
                    .http_only(true)
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
    pub fn create<S>(req: &HttpRequest<S>, user: i32, is_super: bool) {
        let now = Utc::now().naive_utc();
        let new = NewSession {
            user,
            is_super,
            expires: now + Duration::days(MAX_DURATION),
            last_used: now,
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
}

impl<P> std::ops::Deref for Session<P> {
    type Target = DbSession;

    fn deref(&self) -> &DbSession {
        &self.data
    }
}


impl<S, P> FromRequest<S> for Session<P>
where
    P: Policy,
{
    type Config = ();
    type Result = Result<Session<P>, SessionFromRequestError>;

    fn from_request(req: &HttpRequest<S>, _cfg: &()) -> Self::Result {
        if let Some(session) = req.extensions().get::<SessionData>()
                                .and_then(|s| s.existing) {
            if !P::validate(&session) {
                return Err(SessionFromRequestError::Policy);
            }

            Ok(Session {
                data: session.clone(),
                _policy: PhantomData,
            })
        } else {
            Err(SessionFromRequestError::NoSession)
        }
    }
}

impl Policy for Normal {
    fn validate(_: &DbSession) -> bool {
        true
    }
}

impl Policy for Elevated {
    fn validate(session: &DbSession) -> bool {
        session.is_super
    }
}

#[derive(Debug, Fail)]
pub enum SessionFromRequestError {
    #[fail(display = "No session")]
    NoSession,
    #[fail(display = "Unsealing error: {}", _0)]
    Unsealing(#[cause] utils::UnsealingError),
    #[fail(display = "Invalid base64: {}", _0)]
    Decoding(#[cause] base64::DecodeError),
    /// Session was rejected by policy.
    #[fail(display = "Rejected by policy")]
    Policy,
}

impl ResponseError for SessionFromRequestError {
    fn error_response(&self) -> HttpResponse {
        use self::SessionFromRequestError::*;

        match *self {
            NoSession => HttpResponse::Unauthorized()
                .body("a session is required"),
            Unsealing(_) | Decoding(_) => HttpResponse::BadRequest()
                .body("could not decode session cookie"),
            Policy => HttpResponse::Forbidden()
                .body("access denied by policy"),
        }
    }
}
