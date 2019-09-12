use actix_web::{
    Either,
    FromRequest,
    HttpMessage,
    HttpRequest,
    HttpResponse,
    ResponseError,
    dev::Payload,
    error::ErrorUnsupportedMediaType,
    http::{StatusCode, header::ACCEPT_LANGUAGE},
    web::{Form, FormConfig, Json, JsonConfig},
};
use adaptarr_error::{ApiError, Error};
use adaptarr_i18n::{I18n, LanguageRange};
use adaptarr_models::{
    FindModelError,
    Model,
    TeamMember,
    TeamResource,
    db::{Connection, Pool, PooledConnection},
    permissions::{NoPermissions, Permission, PermissionBits, TeamPermissions},
};
use failure::Fail;
use futures::future::{self, Future, FutureResult};
use std::{marker::PhantomData, ops::Deref, str::FromStr};

use crate::session::{Normal, Session};

/// Extract preferred locale from request's headers.
pub struct Locale<'a>(&'a adaptarr_i18n::Locale);

impl<'a> Locale<'a> {
    /// Obtain reference to the underlying [`adaptarr_i18n::Locale`].
    ///
    /// This method differs from [`AsRef::as_ref`] in that lifetime of the
    /// returned reference is not tied to the borrow. This allows for obtaining
    /// static references to locale.
    #[allow(clippy::should_implement_trait)]
    pub fn as_ref(&self) -> &'a adaptarr_i18n::Locale {
        self.0
    }
}

impl<'a> FromRequest for Locale<'a> {
    type Error = Error;
    type Future = FutureResult<Locale<'a>, Error>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let header = req.headers()
            .get(ACCEPT_LANGUAGE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default();

        let locales = parse_accept(header)
            .map(|x| x.0)
            .collect::<Vec<LanguageRange>>();

        match req.app_data::<I18n>() {
            None => future::err(LocaleDataMissingError.into()),
            Some(i18n) => future::ok(Locale(i18n.match_locale(&locales))),
        }
    }
}

impl<'a> Deref for Locale<'a> {
    type Target = adaptarr_i18n::Locale;

    fn deref(&self) -> &adaptarr_i18n::Locale {
        self.0
    }
}

/// Error returned by [`Locale`]'s implementation of [`FromRequest`] when locale
/// data has not been configured.
#[derive(ApiError, Debug, Fail)]
#[api(internal)]
#[fail(display = "locale data (Data<I18n>) needs to be set for Locale \
    extraction to work")]
pub struct LocaleDataMissingError;

/// Parse value of an `Accept-*` header.
fn parse_accept<'s, T>(accept: &'s str) -> impl Iterator<Item = (T, f32)> +'s
where
    T: FromStr + 's,
{
    accept.split(',')
        .map(str::trim)
        .map(|item| -> Result<(T, f32), T::Err> {
            if let Some(inx) = item.find(';') {
                let (item, q) = item.split_at(inx);
                let item = item.trim().parse()?;
                let q = q.trim().parse().unwrap_or(1.0);
                Ok((item, q))
            } else {
                Ok((item.parse()?, 1.0))
            }
        })
        .filter_map(Result::ok)
}

/// Extract a value from the request's body as either form data
/// (`application/x-www-form-urlencoded`), or as JSON.
pub struct FormOrJson<T>(Either<Form<T>, Json<T>>);

impl<T> FormOrJson<T> {
    /// Obtain actual value.
    pub fn into_inner(self) -> T {
        match self.0 {
            Either::A(a) => a.into_inner(),
            Either::B(b) => b.into_inner(),
        }
    }
}

impl<T> std::ops::Deref for FormOrJson<T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self.0 {
            Either::A(ref a) => &*a,
            Either::B(ref b) => &*b,
        }
    }
}

#[derive(Clone, Default)]
pub struct FormOrJsonConfig {
    pub form: FormConfig,
    pub json: JsonConfig,
}

impl<T> FromRequest for FormOrJson<T>
where
    T: serde::de::DeserializeOwned + 'static,

{
    type Error = actix_web::Error;
    type Future = Box<dyn Future<Item = Self, Error = actix_web::Error>>;
    type Config = FormOrJsonConfig;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let mime = match req.mime_type() {
            Ok(mime) => mime,
            Err(err) => return Box::new(future::err(err.into())),
        };

        let is_json = mime.map_or(false, |mime| {
            mime.subtype() == "json" || mime.suffix().map_or(false, |s| s == "json")
        });

        if is_json {
            Box::new(Json::from_request(req, payload).map(Either::B).map(FormOrJson))
        } else if req.content_type().eq_ignore_ascii_case("application/x-www-form-urlencoded") {
            Box::new(Form::from_request(req, payload).map(Either::A).map(FormOrJson))
        } else {
            Box::new(future::err(ErrorUnsupportedMediaType(
                "Body should be application/x-www-form-urlencoded or JSON")))
        }
    }
}

/// Extract a database connection for a request.
pub struct Database(PooledConnection);

impl FromRequest for Database {
    type Error = Error;
    type Future = Result<Database, Error>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let pool = match req.app_data::<Pool>() {
            Some(pool) => pool,
            None => return Err(DatabasePoolMissing.into()),
        };

        pool.get().map_err(Error::from).map(Database)
    }
}

impl Deref for Database {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.0
    }
}

/// Error returned by [`Database`]'s implementation of [`FromRequest`] when
/// connection pool has not been configured.
#[derive(ApiError, Debug, Fail)]
#[api(internal)]
#[fail(display = "database pool needs to be set for Database extraction to work")]
pub struct DatabasePoolMissing;

/// Value of the secret key from a request.
///
/// This structure is designed to work with [`actix_web::web::Data`].
pub struct Secret {
    secret: Box<[u8]>,
}

impl Secret {
    /// Construct a new secret.
    pub fn new(secret: &[u8]) -> Self {
        Secret {
            secret: secret.to_vec().into_boxed_slice(),
        }
    }
}

impl std::ops::Deref for Secret {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.secret
    }
}

/// Limit access to a route to users who are members of a specific team, and
/// have required permissions in that team.
///
/// The first type argument is a [`TeamResource`] to which the access is being
/// limited. Its ID must be parsable from a string, and must be specified in the
/// first fragment of route's path.
///
/// The second type argument is a [`Permission`] describing the set of required
/// permissions. This argument can be omitted to indicate, that no permissions
/// are required.
pub struct TeamScoped<T, P = NoPermissions<TeamPermissions>>
where
    T: TeamResource,
    P: Permission<Bits = TeamPermissions>
{
    resource: T,
    permissions: TeamPermissions,
    _phantom: PhantomData<(*const T, *const P)>
}

impl<T, P> TeamScoped<T, P>
where
    T: TeamResource,
    P: Permission<Bits = TeamPermissions>
{
    /// Get permissions current user has in this scope.
    pub fn permissions(&self) -> TeamPermissions {
        self.permissions
    }

    /// Get reference to the resource this guard is scoped to.
    pub fn resource(&self) -> &T {
        &self.resource
    }

    /// Get the resource this guard is scoped to.
    pub fn into_resource(self) -> T {
        self.resource
    }
}

impl<T, P> FromRequest for TeamScoped<T, P>
where
    T: TeamResource + 'static,
    <T as Model>::Id: FromStr,
    <<T as Model>::Id as FromStr>::Err: Fail,
    P: Permission<Bits = TeamPermissions>,
{
    type Error = actix_web::Error;
    type Future = Result<Self, actix_web::Error>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let resource_id: <T as Model>::Id = req.match_info()[0]
            .parse()
            .map_err(ParsePathError)?;
        let session = Session::<Normal>::extract(req)?;
        let db = Database::extract(req)?;
        let resource = T::by_id(&db, resource_id).map_err(Error::from)?;

        if session.is_elevated {
            return Ok(TeamScoped {
                resource,
                permissions: TeamPermissions::all(),
                _phantom: PhantomData,
            });
        }

        let membership = TeamMember::by_id(
            &db,
            (resource.team_id(), session.user),
        ).map_err(|err| match err {
            FindModelError::NotFound(_) => FindModelError::<T>::not_found(),
            FindModelError::Database(_, err) => FindModelError::<T>::from(err),
        }).map_err(Error::from)?;

        membership.permissions().require(P::bits()).map_err(Error::from)?;

        Ok(TeamScoped {
            resource,
            permissions: membership.permissions(),
            _phantom: PhantomData,
        })
    }
}

#[derive(Debug, Fail)]
#[fail(display = "{}", _0)]
struct ParsePathError<E: Fail>(#[cause] E);

impl<E: Fail> ResponseError for ParsePathError<E> {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::new(StatusCode::BAD_REQUEST)
    }
}
