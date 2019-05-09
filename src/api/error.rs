use actix_web::{
    App,
    FromRequest,
    HttpRequest,
    HttpResponse,
    Responder,
    Scope,
    dev::{AsyncResult, Route},
    http::{StatusCode, Method},
};
use failure::Fail;
use futures::{Async, Future, Poll};
use sentry::{Hub, integrations::failure::event_from_fail};
use sentry_actix::ActixWebHubExt;

/// An error that occurred while handling an API request.
pub trait ApiError: Fail {
    /// HTTP response status code.
    fn status(&self) -> StatusCode;

    /// Internal code describing this error.
    ///
    /// This code is used to identify this error outside the system, and thus
    /// should only be present for errors which are intended to be reported
    /// to the user in detail.
    fn code(&self) -> Option<&str>;
}

/// A wrapper around many types of errors, including user-facing [`ApiError`]s
/// as well as many other errors that should not be reported to the user, such
/// as database connection errors.
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "{}", _0)]
    Api(Box<dyn ApiError>),
    /// Generic system error.
    #[fail(display = "{}", _0)]
    System(#[cause] std::io::Error),
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
    Db(#[cause] diesel::result::Error),
    /// Error obtaining database connection for the pool.
    #[fail(display = "{}", _0)]
    DbPool(#[cause] r2d2::Error),
    /// Error rendering template.
    ///
    /// Note that due to [`tera::Error`] currently being `!Send + !Sync` it
    /// cannot be stored in this enum. Instead we keep its message.
    #[fail(display = "{}", _0)]
    Template(String),
    /// Error sending messages between actors.
    #[fail(display = "{}", _0)]
    ActixMailbox(#[cause] actix::MailboxError),
    /// Error reading message payload.
    #[fail(display = "{}", _0)]
    Payload(#[cause] actix_web::error::PayloadError),
}

impl<T: ApiError> From<T> for Error {
    fn from(error: T) -> Error {
        Error::Api(Box::new(error))
    }
}

impl_from! { for Error ;
    std::io::Error => |e| Error::System(e),
    diesel::result::Error => |e| Error::Db(e),
    r2d2::Error => |e| Error::DbPool(e),
    tera::Error => |e| Error::Template(e.to_string()),
    actix::MailboxError => |e| Error::ActixMailbox(e),
    actix_web::error::PayloadError => |e| Error::Payload(e),
}

#[derive(Debug)]
enum ApiResult<R> {
    Response(R),
    Error(Error),
}

impl<R: Responder> Responder for ApiResult<R> {
    type Item = AsyncResult<HttpResponse>;
    type Error = actix_web::error::Error;

    fn respond_to<S: 'static>(self, req: &HttpRequest<S>)
    -> Result<Self::Item, <Self as Responder>::Error> {
        let err = match self {
            ApiResult::Response(r) => return r.respond_to(req)
                .map(Into::into)
                .map_err(Into::into),
            ApiResult::Error(e) => e,
        };

        capture_error(req, &err);

        match err {
            Error::Api(err) => Ok(AsyncResult::ok({
                if let Some(code) = err.code() {
                    HttpResponse::build(err.status())
                        .json(ErrorResponse {
                            error: code,
                            raw: err.to_string(),
                        })
                } else {
                    error!("{}", err);
                    HttpResponse::new(err.status())
                }
            })),
            Error::Payload(e) => Err(e.into()),
            _ => Ok(AsyncResult::ok({
                error!("{}", err);
                HttpResponse::InternalServerError()
                    .finish()
            })),
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse<'s> {
    error: &'s str,
    raw: String,
}

/// An alternative version of Actix's request handler, that may fail with
/// [`ApiError`]s instead of Actix's [`actix_web::error::Error`].
///
/// To mount an `ApiHandler` you have to use one of [`RouterExt`] methods.
pub trait ApiHandler<Args: FromRequest<S>, S> {
    type Response: Responder;
    type Error: Into<Error>;

    fn handle(&self, args: Args) -> Result<Self::Response, Self::Error>;
}

/// An asynchronous version of [`ApiHandler`].
pub trait ApiAsyncHandler<Args: FromRequest<S>, S> {
    type Response: Responder;
    type Error: Into<Error>;
    type Future: Future<Item = Self::Response, Error = Self::Error>;

    fn handle_async(&self, args: Args) -> Self::Future;
}

macro_rules! impl_api_handler {
    {
        $(
            $($name:ident : $type:ident),*;
        )*
    } => {
        $(
            impl<Func, State, Res, Err $(, $type)*> ApiHandler<($($type,)*), State> for Func
            where
                Func: Fn($($type),*) -> Result<Res, Err>,
                ($($type,)*): FromRequest<State>,
                Res: Responder,
                Err: Into<Error>,
            {
                type Response = Res;
                type Error = Err;

                fn handle(&self, ($($name,)*): ($($type,)*))
                -> Result<Res, Err> {
                    self($($name),*)
                }
            }

            impl<Func, State, Fut, Res, Err $(, $type)*> ApiAsyncHandler<($($type,)*), State> for Func
            where
                Func: Fn($($type),*) -> Fut,
                ($($type,)*): FromRequest<State>,
                Fut: Future<Item = Res, Error = Err>,
                Res: Responder,
                Err: Into<Error>,
            {
                type Response = Res;
                type Error = Err;
                type Future = Fut;

                fn handle_async(&self, ($($name,)*): ($($type,)*)) -> Fut {
                    self($($name),*)
                }
            }
        )*
    }
}

impl_api_handler! {
    ;
    a: A;
    a: A, b: B;
    a: A, b: B, c: C;
    a: A, b: B, c: C, d: D;
    a: A, b: B, c: C, d: D, e: E;
    a: A, b: B, c: C, d: D, e: E, f: F;
    a: A, b: B, c: C, d: D, e: E, f: F, g: G;
    a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H;
    a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I;
    a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J;
}

/// This trait extends Actix's route management with support for [`ApiHandler`].
///
/// For every normal mounting method this trait provides a method with the same
/// name prefixed with `api_` that accepts [`ApiHandler`]s.
pub trait RouterExt<S> {
    fn api_route<T, H>(self, path: &str, method: Method, handler: H) -> Self
    where
        T: FromRequest<S> + 'static,
        H: ApiHandler<T, S> + 'static;
}

impl<S: 'static> RouterExt<S> for App<S> {
    fn api_route<T, H>(self, path: &str, method: Method, handler: H) -> Self
    where
        T: FromRequest<S> + 'static,
        H: ApiHandler<T, S> + 'static,
    {
        self.route(path, method, build_handler(handler))
    }
}

impl<S: 'static> RouterExt<S> for Scope<S> {
    fn api_route<T, H>(self, path: &str, method: Method, handler: H) -> Self
    where
        T: FromRequest<S> + 'static,
        H: ApiHandler<T, S> + 'static,
    {
        self.route(path, method, build_handler(handler))
    }
}

/// This trait extends [`actix_web::dev::Route`] with support
/// for [`ApiHandler`].
///
/// For every normal mounting method this trait provides a method with the same
/// name prefixed with `api_` that accepts [`ApiHandler`]s.
pub trait RouteExt<S> {
    fn api_with<T, H>(&mut self, handler: H)
    where
        T: FromRequest<S> + 'static,
        H: ApiHandler<T, S> + 'static;

    fn api_with_async<T, H>(&mut self, handler: H)
    where
        T: FromRequest<S> + 'static,
        H: ApiAsyncHandler<T, S> + 'static;
}

impl<S: 'static> RouteExt<S> for Route<S> {
    fn api_with<T, H>(&mut self, handler: H)
    where
        T: FromRequest<S> + 'static,
        H: ApiHandler<T, S> + 'static,
    {
        self.with(build_handler(handler))
    }

    fn api_with_async<T, H>(&mut self, handler: H)
    where
        T: FromRequest<S> + 'static,
        H: ApiAsyncHandler<T, S> + 'static,
    {
        self.with_async(build_async_handler(handler))
    }
}

/// Turn an [`ApiHandler`] into an Actix handler.
fn build_handler<S, T, H>(handler: H)
    -> impl Fn(T) -> ApiResult<H::Response>
where
    T: FromRequest<S>,
    H: ApiHandler<T, S>,
{
    move |args| match handler.handle(args) {
        Ok(rsp) => ApiResult::Response(rsp),
        Err(err) => ApiResult::Error(err.into()),
    }
}

/// Turn an [`ApiAsyncHandler`] into an Actix handler.
fn build_async_handler<S, T, H>(handler: H)
    -> impl Fn(T) -> HandlerFuture<S, T, H>
where
    T: FromRequest<S>,
    H: ApiAsyncHandler<T, S>,
{
    move |args| HandlerFuture {
        fut: handler.handle_async(args),
    }
}

struct HandlerFuture<S, T, H>
where
    T: FromRequest<S>,
    H: ApiAsyncHandler<T, S>,
{
    fut: H::Future,
}

impl<S, T, H> Future for HandlerFuture<S, T, H>
where
    T: FromRequest<S>,
    H: ApiAsyncHandler<T, S>
{
    type Item = ApiResult<H::Response>;
    type Error = actix_web::error::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.fut.poll() {
            Ok(Async::Ready(rsp)) => Ok(Async::Ready(ApiResult::Response(rsp))),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Ok(Async::Ready(ApiResult::Error(err.into()))),
        }
    }
}

/// Capture an error and report it to Sentry.io.
fn capture_error<S>(req: &HttpRequest<S>, error: &Error) {
    Hub::from_request(req)
        .capture_event(event_from_fail(error));
}
