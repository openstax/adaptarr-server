use mime::{Mime, Name};
use actix_web::{dev::RequestHead, guard::Guard, http::header::CONTENT_TYPE};

/// Only pass requests with matching `Content-Type` header.
pub struct ContentType<'a>(Name<'a>, Name<'a>);

impl<'a> ContentType<'a> {
    pub fn from_mime(mime: &'a Mime) -> Self {
        ContentType(mime.type_(), mime.subtype())
    }
}

impl<'a> Guard for ContentType<'a> {
    fn check(&self, req: &RequestHead) -> bool {
        match req.headers
            .get(CONTENT_TYPE)
            .map(|v| v.to_str().map(str::parse::<Mime>))
        {
            Some(Ok(Ok(mime))) =>
                mime.type_() == self.0 && mime.subtype() == self.1,
            _ => false,
        }
    }
}
