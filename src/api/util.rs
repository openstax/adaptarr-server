use actix_web::{
    FromRequest,
    HttpRequest,
    http::header::ACCEPT_LANGUAGE,
};
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
