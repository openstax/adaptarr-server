use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use adaptarr_macros::From;
use failure::Fail;
use log::error;
use serde::Serialize;
use std::borrow::Cow;

pub use adaptarr_macros::ApiError;

/// An error that occurred while handling an API request.
pub trait ApiError: Fail {
    /// HTTP response status code.
    fn status(&self) -> StatusCode;

    /// Internal code describing this error.
    ///
    /// This code is used to identify this error outside the system, and thus
    /// should only be present for errors which are intended to be reported
    /// to the user in detail.
    fn code(&self) -> Option<Cow<str>>;
}

/// This implementation is required to make `#[cause]` on a `Box<dyn ApiError>`
/// work.
impl Fail for Box<dyn ApiError> {
    fn name(&self) -> Option<&str> {
        (**self).name()
    }

    fn cause(&self) -> Option<&dyn Fail> {
        (**self).cause()
    }

    fn backtrace(&self) -> Option<&failure::Backtrace> {
        (**self).backtrace()
    }
}

/// A wrapper around many types of errors, including user-facing [`ApiError`]s
/// as well as many other errors that should not be reported to the user, such
/// as database connection errors.
#[derive(Debug, Fail, From)]
pub enum Error {
    #[fail(display = "{}", _0)]
    Api(#[cause] Box<dyn ApiError>),
    /// Generic system error.
    #[fail(display = "{}", _0)]
    System(#[cause] #[from] std::io::Error),
    /// Error communicating with the database.
    ///
    /// Note that this variant also includes errors related to missing record,
    /// you may want to turn them into [`ApiError`]s instead:
    ///
    /// ```ignore
    /// database_operation
    ///     .optional()?
    ///     .ok_or_else(|| MyApiError::NotFound)?
    /// ```
    #[fail(display = "{}", _0)]
    Db(#[cause] #[from] diesel::result::Error),
    /// Error obtaining database connection for the pool.
    #[fail(display = "{}", _0)]
    DbPool(#[cause] #[from] r2d2::Error),
    /// Error rendering template.
    ///
    /// Note that due to [`tera::Error`] currently being `!Send + !Sync` it
    /// cannot be stored in this enum. Instead we keep its message.
    #[fail(display = "{}", _0)]
    Template(String),
    /// Error sending messages between actors.
    #[fail(display = "{}", _0)]
    ActixMailbox(#[cause] #[from] actix::MailboxError),
    /// Error reading message payload.
    #[fail(display = "{}", _0)]
    Payload(#[from] actix_web::error::PayloadError),
    /// Error generating a URL.
    #[fail(display = "{}", _0)]
    UrlGeneration(#[from] actix_web::error::UrlGenerationError),
}

impl<T: ApiError> From<T> for Error {
    fn from(error: T) -> Error {
        Error::Api(Box::new(error))
    }
}

impl From<tera::Error> for Error {
    fn from(e: tera::Error) -> Self {
        let mut msg = String::new();
        for (inx, err) in e.iter().enumerate() {
            if inx > 0 {
                msg.push_str(": ");
            }
            msg.push_str(&err.to_string());
        }
        Error::Template(msg)
    }
}

impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        match self {
            Error::Api(err) => match err.code() {
                Some(code) => HttpResponse::build(err.status())
                    .json(ErrorResponse {
                        error: code,
                        raw: err.to_string(),
                    }),
                None => {
                    error!("{}", err);
                    HttpResponse::new(err.status())
                }
            },
            Error::Payload(e) => e.error_response(),
            _ => {
                error!("{}", self);
                HttpResponse::InternalServerError()
                    .finish()
            }
        }
    }

    fn render_response(&self) -> HttpResponse {
        self.error_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse<'s> {
    error: Cow<'s, str>,
    raw: String,
}
