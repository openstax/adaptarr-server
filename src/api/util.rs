use actix_web::{
    Either,
    Form,
    FromRequest,
    HttpRequest,
    HttpResponse,
    Json,
    Responder,
    http::{
        HttpTryFrom,
        StatusCode,
        header::{ACCEPT_LANGUAGE, LOCATION, HeaderValue},
    },
};
use futures::Future;
use std::str::FromStr;

use crate::i18n::{LanguageRange, Locale};
use super::State;

/// Parse value of an `Accept-*` header.
pub fn parse_accept<'s, T>(accept: &'s str) -> impl Iterator<Item = (T, f32)> +'s
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

impl<'a> FromRequest<State> for &'a Locale<'static> {
    type Config = ();
    type Result = &'a Locale<'static>;

    fn from_request(req: &HttpRequest<State>, _: &()) -> &'a Locale<'static> {
        let header = req.headers()
            .get(ACCEPT_LANGUAGE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default();
        let locales = parse_accept(header)
            .map(|x| x.0)
            .collect::<Vec<LanguageRange>>();

        debug!("Accept-Language: {:?}", locales);

        req.state().i18n.match_locale(&locales)
    }
}

pub struct FormOrJson<T>(Either<Form<T>, Json<T>>);

impl<T> FormOrJson<T> {
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

impl<S, T> FromRequest<S> for FormOrJson<T>
where
    T: serde::de::DeserializeOwned + 'static,
    S: 'static,
{
    type Config = <Either<Form<T>, Json<T>> as FromRequest<S>>::Config;
    type Result = Box<dyn Future<Item = Self, Error = actix_web::Error>>;

    fn from_request(req: &HttpRequest<S>, config: &Self::Config) -> Self::Result {
        Box::new(Either::from_request(req, config).map(FormOrJson))
    }
}

pub struct WithStatus<T>(pub StatusCode, pub T);

impl<T: Responder + 'static> Responder for WithStatus<T> {
    type Item = Box<dyn Future<Item = HttpResponse, Error = actix_web::Error>>;
    type Error = <T as Responder>::Error;

    fn respond_to<S: 'static>(self, req: &HttpRequest<S>)
    -> Result<Self::Item, Self::Error> {
        let WithStatus(code, responder) = self;

        Ok(Box::new(responder.respond_to(req)?.into().map(move |mut rsp| {
            *rsp.status_mut() = code;
            rsp
        })))
    }
}

pub struct Created<L, T>(pub L, pub T);

impl<L, T> Responder for Created<L, T>
where
    T: Responder + 'static,
    L: 'static,
    HeaderValue: HttpTryFrom<L>,
    <HeaderValue as HttpTryFrom<L>>::Error: actix_web::ResponseError,
{
    type Item = Box<dyn Future<Item = HttpResponse, Error = actix_web::Error>>;
    type Error = <T as Responder>::Error;

    fn respond_to<S: 'static>(self, req: &HttpRequest<S>)
    -> Result<Self::Item, Self::Error> {
        let Created(location, responder) = self;

        Ok(Box::new(responder.respond_to(req)?.into().and_then(move |mut rsp| {
            *rsp.status_mut() = StatusCode::CREATED;
            rsp.headers_mut().insert(LOCATION, HeaderValue::try_from(location)?);
            Ok(rsp)
        })))
    }
}
