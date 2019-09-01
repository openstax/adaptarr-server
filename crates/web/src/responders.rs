use actix_web::{
    HttpRequest,
    HttpResponse,
    Responder,
    http::{HttpTryFrom, StatusCode, header::{LOCATION, HeaderValue}},
};
use futures::future::{Future, IntoFuture};

/// Build a 201 Created response.
///
/// The `Location` header is defined by the first field, and remaining
/// properties of the response (including its body) by [`Responder`] in
/// the second field.
pub struct Created<L, T>(pub L, pub T);

impl<L, T> Responder for Created<L, T>
where
    T: Responder + 'static,
    L: 'static,
    HeaderValue: HttpTryFrom<L>,
    actix_web::Error: From<<HeaderValue as HttpTryFrom<L>>::Error>,
{
    type Future = Box<dyn Future<Item = HttpResponse, Error = Self::Error>>;
    type Error = actix_web::Error;

    fn respond_to(self, req: &HttpRequest) -> Self::Future {

        let Created(location, responder) = self;

        Box::new(responder.respond_to(req)
            .into_future()
            .map_err(Into::into)
            .and_then(move |mut rsp| {
                *rsp.status_mut() = StatusCode::CREATED;
                rsp.headers_mut().insert(LOCATION, HeaderValue::try_from(location)?);
                Ok(rsp)
            }))
    }
}

/// Change status code of a response.
pub struct WithStatus<T>(pub StatusCode, pub T);

impl<T: Responder + 'static> Responder for WithStatus<T> {
    type Future = Box<dyn Future<Item = HttpResponse, Error = Self::Error>>;
    type Error = <T as Responder>::Error;

    fn respond_to(self, req: &HttpRequest) -> Self::Future {

        let WithStatus(code, responder) = self;

        Box::new(responder.respond_to(req).into_future().map(move |mut rsp| {
            *rsp.status_mut() = code;
            rsp
        }))
    }
}
