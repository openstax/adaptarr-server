//! Parsing `multipart/form-data`.

use actix_multipart::Field;
use actix_web::{
    FromRequest,
    HttpRequest,
    HttpResponse,
    ResponseError,
    dev::Payload,
};
use adaptarr_macros::From;
use bytes::Bytes;
use failure::Fail;
use futures::{Future, Stream, future::self};
use std::{io::Write, str::FromStr};
use tempfile::{Builder as TempBuilder, NamedTempFile};

pub use adaptarr_macros::FromMultipart;

/// Trait for types which can be loaded from a `multipart/form-data` request.
pub trait FromMultipart: Sized {
    type Error;
    type Result: Future<Item = Self, Error = Self::Error>;

    fn from_multipart<S, F>(fields: S) -> Self::Result
    where
        S: Stream<Item = (String, F), Error = MultipartError> + 'static,
        F: Stream<Item = Bytes, Error = MultipartError> + 'static;
}

/// Trait for types which can be loaded from fields of a `multipart/form-data`
/// request.
pub trait FromField: Sized {
    type Error;
    type Result: Future<Item = Self, Error = Self::Error>;

    fn from_field<S>(field: S) -> Self::Result
    where
        S: Stream<Item = Bytes, Error = MultipartError> + 'static;

    /// Default value of this filed, if it was not present in multipart,
    /// or `None` if the field must be present.
    fn default() -> Option<Self> {
        None
    }
}

/// Wraps a type implementing [`FromMultipart`] providing
/// [`actix_web::FromRequest`].
#[derive(Debug)]
pub struct Multipart<T> {
    inner: T,
}

impl<T> Multipart<T> {
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> FromRequest for Multipart<T>
where
    T: FromMultipart + 'static,
    <T as FromMultipart>::Result: 'static,
    actix_web::Error: From<<T as FromMultipart>::Error>,
{
    type Error = actix_web::Error;
    type Future = Box<dyn Future<Item = Self, Error = actix_web::Error>>;
    type Config = ();

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let stream = match actix_multipart::Multipart::from_request(req, payload) {
            Ok(stream) => stream,
            Err(err) => return Box::new(future::err(err)),
        };

        let items = stream
            .from_err::<MultipartError>()
            .and_then(process_field)
            .map(|(name, body)| (
                name,
                body.from_err::<MultipartError>(),
            ));

        Box::new(<T as FromMultipart>::from_multipart(items)
            .from_err()
            .map(|inner| Multipart { inner }))
    }
}

#[derive(Debug, Fail, From)]
pub enum MultipartError {
    #[fail(display = "Field is missing Content-Disposition header")]
    ContentDispositionMissing,
    #[fail(display = "Field is not a form data")]
    NotFormData,
    #[fail(display = "Field has no name")]
    UnnamedField,
    #[fail(display = "Missing required field {}", _0)]
    FieldMissing(&'static str),
    #[fail(display = "Unexpected field {:?}", _0)]
    UnexpectedField(String),
    #[fail(display = "Multipart error: {}", _0)]
    Multipart(#[from] actix_multipart::MultipartError),
    #[fail(display = "Bad data: {}", _0)]
    BadData(#[cause] Box<dyn failure::Fail>),
    #[fail(display = "{}", _0)]
    Internal(#[cause] Box<dyn failure::Fail>),
}

impl ResponseError for MultipartError {
    fn error_response(&self) -> HttpResponse {
        use self::MultipartError::*;

        match *self {
            ContentDispositionMissing | NotFormData | UnnamedField
            | FieldMissing(_) | UnexpectedField(_) =>
                HttpResponse::BadRequest().body(self.to_string()),
            Multipart(ref e) => e.error_response(),
            BadData(ref e) => HttpResponse::BadRequest().body(e.to_string()),
            Internal(_) => HttpResponse::InternalServerError().finish(),
        }
    }
}

fn process_field(field: Field) -> impl Future<Item = (String, Field), Error = MultipartError> {
    let cd = match field.content_disposition() {
        Some(cd) => cd,
        None => return future::err(MultipartError::ContentDispositionMissing),
    };

    if !cd.is_form_data() {
        return future::err(MultipartError::NotFormData);
    }

    future::result({
        cd.get_name()
            .ok_or(MultipartError::UnnamedField)
            .map(|name| (name.to_string(), field))
    })
}

impl<F: FromField + 'static> FromField for Option<F> {
    type Error = F::Error;
    type Result = Box<dyn Future<Item = Self, Error = Self::Error>>;

    fn from_field<S>(field: S) -> Self::Result
    where
        S: Stream<Item = Bytes, Error = MultipartError> + 'static,
    {
        Box::new(F::from_field(field).map(Some))
    }

    fn default() -> Option<Option<F>> {
        Some(None)
    }
}

impl FromField for String {
    type Error = MultipartError;
    type Result = Box<dyn Future<Item = Self, Error = Self::Error>>;

    fn from_field<S>(field: S) -> Self::Result
    where
        S: Stream<Item = Bytes, Error = MultipartError> + 'static,
    {
        Box::new(field
            .fold(Vec::with_capacity(1024), |mut value, chunk| {
                value.extend_from_slice(&chunk);
                future::ok::<_, MultipartError>(value)
            })
            .and_then(|v| {
                future::result(String::from_utf8(v))
                    .map_err(|e| MultipartError::BadData(Box::new(e)))
            }))
    }
}

impl FromField for NamedTempFile {
    type Error = MultipartError;
    type Result = Box<dyn Future<Item = Self, Error = Self::Error>>;

    fn from_field<S>(field: S) -> Self::Result
    where
        S: Stream<Item = Bytes, Error = MultipartError> + 'static,
    {
        let storage_path = &adaptarr_models::Config::global().storage.path;

        Box::new(future::result(TempBuilder::new().tempfile_in(storage_path))
            .map_err(|e| MultipartError::Internal(Box::new(e)))
            .and_then(|file| field.fold(file, |mut file, chunk| {
                match file.write_all(chunk.as_ref()) {
                    Ok(_) => future::ok(file),
                    Err(e) => future::err(MultipartError::Internal(Box::new(e))),
                }
            })))
    }
}

pub struct FromStrField<T>(T);

impl<T> FromStrField<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for FromStrField<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> FromField for FromStrField<T>
where
    T: FromStr + 'static,
    <T as FromStr>::Err: Fail,
{
    type Error = MultipartError;
    type Result = Box<dyn Future<Item = Self, Error = Self::Error>>;

    fn from_field<S>(field: S) -> Self::Result
    where
        S: Stream<Item = Bytes, Error = MultipartError> + 'static,
    {
        Box::new(String::from_field(field).and_then(|s| match T::from_str(&s) {
            Ok(v) => future::ok(FromStrField(v)),
            Err(err) => future::err(MultipartError::BadData(Box::new(err))),
        }))
    }
}
