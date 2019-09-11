//! Session management.

use actix_web::{
    HttpRequest,
    HttpMessage,
    FromRequest,
    cookie::SameSite,
    dev::{Payload, Service, ServiceRequest, ServiceResponse, Transform},
    error::{ErrorInternalServerError, Result},
    http::Cookie,
};
use adaptarr_error::{ApiError, Error};
use adaptarr_models::{
    audit,
    db::{
        self,
        Connection,
        Pool,
        models::{Session as DbSession, NewSession, SessionUpdate},
        schema::sessions,
    },
    models::{AssertExists, FindModelError, Model, User},
    permissions::{Permission, PermissionBits, RequirePermissionsError},
};
use chrono::{Duration, Utc};
use diesel::{prelude::*, result::{Error as DbError}};
use failure::Fail;
use futures::{Future, Poll, future::{self, FutureResult}};
use log::debug;
use std::{marker::PhantomData, rc::Rc};

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

#[derive(Clone)]
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
    pub fn new(secret: &[u8]) -> SessionManager {
        SessionManager {
            secret: secret.to_vec(),
            db: db::pool(),
        }
    }

    fn validate(ses: &DbSession) -> Validation {
        let now = Utc::now();

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

    fn before_request(&self, req: &mut ServiceRequest) -> Result<()> {
        let cookie = match req.cookie(COOKIE) {
            Some(cookie) => cookie,
            None => return Ok(()),
        };

        let mut data = match base64::decode(cookie.value()) {
            Ok(data) => data,
            Err(_) => return Ok(()),
        };
        let sesid: i32 = match adaptarr_util::unseal(&self.secret, &mut data) {
            Ok(sesid) => sesid,
            Err(_) => return Ok(()),
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
            None => return Ok(()),
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

        Ok(())
    }

    fn after_request<B>(&self, rsp: &mut ServiceResponse<B>) -> Result<()> {
        let cookie = rsp.request().extensions().get::<SessionData>().map(|session| -> Result<Option<Cookie>> {
            audit::set_actor(None);

            let now = Utc::now();
            let db = self.db.get()
                .map_err(|e| ErrorInternalServerError(e.to_string()))?;

            if session.existing.is_some() && session.destroy {
                diesel::delete(&session.existing.unwrap())
                    .execute(&*db)
                    .map_err(|e| ErrorInternalServerError(e.to_string()))?;
                Ok(Some(Cookie::build(COOKIE, "")
                    .domain(rsp.request().app_config().host().to_string())
                    .path("/")
                    .max_age_time(Duration::zero())
                    .secure(!cfg!(debug_assertions))
                    .http_only(!cfg!(debug_assertions))
                    .same_site(SameSite::Strict)
                    .finish()))
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

                let value = adaptarr_util::seal(&self.secret, session.id)
                    .expect("sealing session ID");
                let cookie = Cookie::build(COOKIE, base64::encode(&value))
                    .domain(rsp.request().app_config().host().to_string())
                    .path("/")
                    .max_age_time(Duration::days(MAX_DURATION))
                    .secure(!cfg!(debug_assertions))
                    .http_only(!cfg!(debug_assertions))
                    .same_site(SameSite::Strict)
                    .finish();
                Ok(Some(cookie))
            } else if let Some(session) = session.existing {
                diesel::update(&session)
                    .set(sessions::last_used.eq(now))
                    .execute(&*db)
                    .map_err(|e| ErrorInternalServerError(e.to_string()))?;
                Ok(None)
            } else {
                Ok(None)
            }
        }).transpose()?;

        if let Some(Some(cookie)) = cookie {
            rsp.response_mut().add_cookie(&cookie)?;
        }

        Ok(())
    }
}

impl<S, B> Transform<S> for SessionManager
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>>,
    S::Error: From<actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = S::Error;
    type Transform = SessionMiddleware<S>;
    type InitError = ();
    type Future = FutureResult<SessionMiddleware<S>, ()>;

    fn new_transform(&self, service: S) -> Self::Future {
        future::ok(SessionMiddleware {
            service,
            manager: Rc::new(self.clone()),
        })
    }
}

pub struct SessionMiddleware<S> {
    service: S,
    manager: Rc<SessionManager>,
}

impl<S, B> Service for SessionMiddleware<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>>,
    S::Error: From<actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = S::Error;
    type Future = Box<dyn Future<Item = Self::Response, Error = S::Error>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, mut req: ServiceRequest) -> Self::Future {
        if let Err(e) = self.manager.before_request(&mut req) {
            return Box::new(future::err(From::from(e)));
        }

        let manager = self.manager.clone();
        Box::new(self.service.call(req)
            .map(move |rsp| rsp.checked_expr(|rsp| manager.after_request(rsp))))
        // Box::new(self.service.call(req))
    }
}

impl<P> Session<P> {
    pub fn create(req: &HttpRequest, user: &User, is_elevated: bool) {
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

        let now = Utc::now();
        let new = NewSession {
            user: user.id,
            is_elevated,
            expires: now + Duration::days(MAX_DURATION),
            last_used: now,
            permissions: (permissions & mask).bits(),
        };

        let mut extensions = req.extensions_mut();

        debug!("creating session");
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

    pub fn destroy(req: &HttpRequest, sess: Self) {
        req.extensions_mut().insert(SessionData {
            existing: Some(sess.data),
            new: None,
            destroy: true,
        })
    }

    pub fn user_id(&self) -> i32 {
        self.data.user
    }

    pub fn user(&self, db: &Connection) -> Result<User, DbError> {
        User::by_id(db, self.data.user).map_err(FindModelError::assert_exists)
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


impl<P> FromRequest for Session<P>
where
    P: Policy,
    Error: From<P::Error>,
{
    type Error = Error;
    type Future = Result<Session<P>, Error>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if let Some(session) = req.extensions().get::<SessionData>()
                                .and_then(|s| s.existing) {

            match P::validate(&session) {
                Validation::Pass => (),
                Validation::Update(update, pass) => {
                    let db = req.app_data::<db::Pool>()
                        .expect("Missing state data")
                        .get()?;

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
    Unsealing(#[cause] adaptarr_util::UnsealingError),
    #[api(internal)]
    #[fail(display = "Invalid base64: {}", _0)]
    Decoding(#[cause] base64::DecodeError),
    /// Session was rejected by policy.
    #[api(status = "FORBIDDEN", code = "user:session:rejected")]
    #[fail(display = "Rejected by policy")]
    Policy,
}
