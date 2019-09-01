use actix_files::NamedFile;
use actix_web::{
    HttpRequest,
    HttpResponse,
    Responder,
    http::header::{ETAG, ContentDisposition, IntoHeaderValue},
};
use adaptarr_models::File;
use adaptarr_util::bytes_to_hex;
use std::path::Path;

use crate::etag::EntityTag;

pub trait FileExt {
    /// Get an Actix responder streaming contents of this file.
    fn stream<P>(&self, storage_path: P) -> std::io::Result<Stream>
    where
        P: AsRef<Path>;

    /// Get an entity tag for the current version of this file.
    fn entity_tag(&self) -> EntityTag<'static>;
}

impl FileExt for File {
    /// Get an Actix responder streaming contents of this file.
    fn stream<P>(&self, storage_path: P) -> std::io::Result<Stream>
    where
        P: AsRef<Path>,
    {
        Stream::open(self, storage_path)
    }

    fn entity_tag(&self) -> EntityTag<'static> {
        // Base64 encoding only uses bytes allowed in entity tags
        EntityTag::strong(base64::encode(&self.hash)).unwrap()
    }
}

pub struct Stream {
    stream: NamedFile,
    hash: Vec<u8>,
}

impl Stream {
    fn open<P>(file: &File, storage_path: P) -> std::io::Result<Stream>
    where
        P: AsRef<Path>,
    {
        let hash = bytes_to_hex(&file.hash);
        let path = storage_path.as_ref().join(hash);
        let mime = file.mime.parse().expect("invalid mime type in database");
        let stream = NamedFile::open(path)?.set_content_type(mime);

        Ok(Stream {
            stream,
            hash: file.hash.clone(),
        })
    }

    pub fn set_content_disposition(mut self, cd: ContentDisposition) -> Self {
        self.stream = self.stream.set_content_disposition(cd);
        self
    }
}

impl Responder for Stream {
    type Error = <NamedFile as Responder>::Error;
    type Future = Result<HttpResponse, Self::Error>;

    fn respond_to(self, req: &HttpRequest) -> Self::Future {
        let mut rsp = self.stream.respond_to(req)?;

        let etag = format!(r#""{}""#, base64::encode(&self.hash));
        rsp.headers_mut().insert(ETAG, etag.try_into().unwrap());

        Ok(rsp)
    }
}
