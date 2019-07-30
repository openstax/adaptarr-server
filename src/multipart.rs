//! Parsing `multipart/form-data`.

use actix_web::{
    FromRequest,
    HttpMessage,
    HttpRequest,
    HttpResponse,
    ResponseError,
    multipart::{Field, MultipartItem},
    error::PayloadError,
};
use bytes::Bytes;
use failure::Fail;
use futures::{Future, Stream, future::self};
use std::{io::Write, str::FromStr};
use tempfile::{Builder as TempBuilder, NamedTempFile};

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

/// Macro that auto-implements [`FromMultipart`] for types.
macro_rules! from_multipart {
    {
        multipart $name:ident via $impl_struct:ident {
            $(
                $field:ident : $type:ty
            ),*
            $(,)*
        }
    } => {
        struct $impl_struct {
            $($field : Option<$type>),*
        }

        impl crate::multipart::FromMultipart for $name
        where
            $(
                $type: crate::multipart::FromField,
                <$type as crate::multipart::FromField>::Error:
                    Into<crate::multipart::MultipartError>,
            )*
        {
            type Error = crate::multipart::MultipartError;
            type Result = Box<dyn futures::Future<
                Item = Self,
                Error = Self::Error,
            >>;

            fn from_multipart<S, F>(fields: S) -> Self::Result
            where
                S: futures::Stream<
                    Item = (String, F),
                    Error = crate::multipart::MultipartError,
                > + 'static,
                F: futures::Stream<
                    Item = bytes::Bytes,
                    Error = crate::multipart::MultipartError,
                > + 'static,
            {
                use futures::{Future, future};

                let data = $impl_struct {
                    $($field: None),*
                };

                Box::new(
                    fields
                    .fold(data, |mut data, (name, body)| {
                        let f: Box<dyn Future<Item = $impl_struct, Error = crate::multipart::MultipartError>> = match name.as_str() {
                            $(
                                stringify!($field) => Box::new(
                                    <$type as crate::multipart::FromField>
                                        ::from_field(body)
                                        .map(|value| {
                                            data.$field = Some(value);
                                            data
                                        })
                                ),
                            )*
                            _ => Box::new(
                                future::err(
                                    crate::multipart::MultipartError
                                        ::UnexpectedField(name))),
                        };
                        f
                    })
                    .map(|data| {
                        let $impl_struct { $($field),* } = data;

                        $(
                            let $field = $field
                                .or_else(crate::multipart::FromField::default)
                                .ok_or(
                                    crate::multipart::MultipartError::FieldMissing(
                                        stringify!($field)))?;
                        )*

                        Ok(Self { $($field),* })
                    })
                    .and_then(future::result)
                )
            }
        }
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

impl<S, T> FromRequest<S> for Multipart<T>
where
    T: FromMultipart,
    <T as FromMultipart>::Result: 'static,
    actix_web::Error: From<<T as FromMultipart>::Error>,
{
    type Config = ();
    type Result = Box<dyn Future<Item = Self, Error = actix_web::Error>>;

    fn from_request(req: &HttpRequest<S>, _: &Self::Config) -> Self::Result {
        let items = req.multipart()
            .from_err::<MultipartError>()
            .map(process_item)
            .flatten()
            .map(|(name, body)| (
                name,
                body.from_err::<MultipartError>(),
            ));

        Box::new(<T as FromMultipart>::from_multipart(items)
            .from_err()
            .map(|inner| Multipart { inner }))
    }
}

#[derive(Debug, Fail)]
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
    #[fail(display = "Bad data: {}", _0)]
    BadData(#[cause] Box<dyn failure::Fail>),
    #[fail(display = "Internal error")]
    Internal(#[cause] Box<dyn failure::Fail>),
}

impl From<actix_web::error::MultipartError> for MultipartError {
    fn from(e: actix_web::error::MultipartError) -> Self {
        MultipartError::BadData(Box::new(e))
    }
}

impl ResponseError for MultipartError {
    fn error_response(&self) -> HttpResponse {
        use self::MultipartError::*;

        match *self {
            ContentDispositionMissing | NotFormData | UnnamedField
            | FieldMissing(_) | UnexpectedField(_) =>
                HttpResponse::BadRequest().body(self.to_string()),
            BadData(ref e) => HttpResponse::BadRequest().body(e.to_string()),
            Internal(_) => HttpResponse::InternalServerError().finish(),
        }
    }
}

/// Process a single multipart item into a stream of fields.
fn process_item<S>(item: MultipartItem<S>)
    -> Box<dyn Stream<Item = (String, Field<S>), Error = MultipartError>>
where
    S: Stream<Item = Bytes, Error = PayloadError> + 'static,
{
    match item {
        MultipartItem::Field(field) => {
            let cd = match field.content_disposition() {
                Some(cd) => cd,
                None => return Box::new(
                    future::err(MultipartError::ContentDispositionMissing)
                        .into_stream()),
            };

            if !cd.is_form_data() {
                return Box::new(
                    future::err(MultipartError::NotFormData)
                        .into_stream());
            }

            Box::new(
                future::result({
                    cd.get_name()
                        .ok_or(MultipartError::UnnamedField)
                        .map(|name| (name.to_string(), field))
                })
                .into_stream()
            )
        }
        MultipartItem::Nested(mp) => Box::new(mp.map(process_item).flatten()),
    }
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
        let config = crate::config::load()
            .expect("configuration should be loaded at this point");

        Box::new(future::result(TempBuilder::new().tempfile_in(&config.storage.path))
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
