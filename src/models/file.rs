use adaptarr_macros::From;
use actix_web::{
    HttpRequest,
    HttpResponse,
    Responder,
    fs::NamedFile,
    http::header::{ETAG, ContentDisposition, IntoHeaderValue},
};
use blake2::blake2b::{Blake2b, Blake2bResult};
use diesel::{
    prelude::*,
    result::Error as DbError,
};
use failure::Fail;
use futures::{Future, Stream as _, future};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use tempfile::{Builder as TempBuilder, NamedTempFile};

use crate::{
    ApiError,
    api::util::EntityTag,
    config::{Config, Storage},
    db::{
        Connection,
        Pool,
        models as db,
        schema::files,
    },
    utils::bytes_to_hex,
};

thread_local! {
    static MAGIC: magic::Cookie = {
        let cookie = magic::Cookie::open(magic::flags::MIME_TYPE)
            .expect("libmagic to initialize");
        cookie.load(&["/usr/share/misc/magic"])
            .expect("libmagic to load database");
        cookie
    };
}

/// MIME-type of a CNXML file.
pub static CNXML_MIME: &str = "application/vnd.openstax.cnx+xml";

/// A virtual file.
#[derive(Debug)]
pub struct File {
    data: db::File,
}

impl File {
    /// Construct `File` from its database counterpart.
    pub(super) fn from_db(data: db::File) -> File {
        File { data }
    }

    /// Find a file by ID.
    pub fn by_id(dbconn: &Connection, id: i32) -> Result<File, FindFileError> {
        files::table
            .filter(files::id.eq(id))
            .get_result::<db::File>(dbconn)
            .optional()?
            .ok_or(FindFileError::NotFound)
            .map(File::from_db)
    }

    /// Create new file from a stream of bytes.
    pub fn from_stream<S, I, E>(
        dbpool: Pool,
        storage: PathBuf,
        data: S,
        mime: Option<&'static str>,
    ) -> impl Future<Item=File, Error=E>
    where
        S: futures::Stream<Item=I>,
        I: AsRef<[u8]>,
        E: From<CreateFileError>,
        E: From<S::Error>,
        E: From<io::Error>,
    {
        future::result(TempBuilder::new().tempfile_in(&storage))
            .map_err(E::from)
            .and_then(|tmp| copy_hash(64, data, tmp))
            .and_then(move |(hash, tmp)| future::result(
                dbpool.get()
                    .map_err(Into::into)
                    .and_then(|db|
                        File::from_file_with_hash(&*db, storage, tmp, hash, mime))
                    .map_err(E::from)))
    }

    /// Create new file from a data in memory.
    pub fn from_data<'c, B>(
        dbconn: &Connection,
        config: &Config,
        data: B,
        mime: Option<&str>,
    ) -> Result<File, CreateFileError>
    where
        B: AsRef<[u8]>,
    {
        let mut hash = Blake2b::new(64);
        hash.update(data.as_ref());
        let hash = hash.finalize();

        match files::table
            .filter(files::hash.eq(hash.as_bytes()))
            .get_result::<db::File>(dbconn)
            .optional()?
        {
            // There already is a file with this hash.
            Some(data) => Ok(File { data }),
            // It's a new file; we need to create database entry for it.
            None => {
                let name = bytes_to_hex(hash.as_bytes());
                let path = config.storage.path.join(name);

                let mut file = std::fs::File::create(&path)?;
                file.write_all(data.as_ref())?;

                let magic = match mime {
                    Some(_) => None,
                    None => Some(MAGIC.with(|magic| magic.file(&path))
                        .expect("libmagic to work")),
                };

                let mime = mime.or(magic.as_ref().map(String::as_str)).unwrap();

                diesel::insert_into(files::table)
                    .values(db::NewFile {
                        mime,
                        path: path.to_str().expect("invalid path"),
                        hash: hash.as_bytes(),
                    })
                    .get_result::<db::File>(dbconn)
                    .map_err(Into::into)
                    .map(|data| File { data })
            },
        }
    }

    /// Create new file from any type implementing [`std::io::Write`].
    pub fn from_read<'c, R>(
        dbconn: &Connection,
        config: &Storage,
        mut read: R,
        mime: Option<&str>,
    ) -> Result<File, CreateFileError>
    where
        R: Read,
    {
        let mut tmp = NamedTempFile::new_in(&config.path)?;

        let digest = {
            let mut hash = HashWriter::new(64, &mut tmp);
            io::copy(&mut read, &mut hash)?;
            hash.finalize()
        };

        File::from_file_with_hash(dbconn, &config.path, tmp, digest, mime)
    }

    /// Create a new file from a temporary file.
    pub fn from_temporary(
        dbconn: &Connection,
        config: &Storage,
        mut file: NamedTempFile,
        mime: Option<&str>,
    ) -> Result<File, CreateFileError> {
        let digest = {
            let mut sink = io::sink();
            let mut hash = HashWriter::new(64, &mut sink);
            file.seek(SeekFrom::Start(0))?;
            io::copy(&mut file, &mut hash)?;
            hash.finalize()
        };

        File::from_file_with_hash(dbconn, &config.path, file, digest, mime)
    }

    /// Create new file from a temporary file and hash.
    ///
    /// This is an internal constructor.
    fn from_file_with_hash<'c, P>(
        dbconn: &Connection,
        storage: P,
        file: NamedTempFile,
        hash: Blake2bResult,
        mime: Option<&str>,
    ) -> Result<File, CreateFileError>
    where
        P: AsRef<Path>,
    {
        match files::table
            .filter(files::hash.eq(hash.as_bytes()))
            .get_result::<db::File>(dbconn)
            .optional()?
        {
            // There already is a file with this hash.
            Some(data) => Ok(File { data }),
            // It's a new file; we need to create database entry for it.
            None => {
                let name = bytes_to_hex(hash.as_bytes());
                let path = storage.as_ref().join(name);
                let _ = file.persist(&path)?;

                let magic = match mime {
                    Some(_) => None,
                    None => Some(MAGIC.with(|magic| magic.file(&path))
                        .expect("libmagic to work")),
                };

                let mime = mime.or(magic.as_ref().map(String::as_str)).unwrap();

                diesel::insert_into(files::table)
                    .values(db::NewFile {
                        mime,
                        path: path.to_str().expect("invalid path"),
                        hash: hash.as_bytes(),
                    })
                    .get_result::<db::File>(dbconn)
                    .map_err(Into::into)
                    .map(|data| File { data })
            }
        }
    }

    /// Unpack database data.
    pub fn into_db(self) -> db::File {
        self.data
    }

    /// Get an Actix responder streaming contents of this file.
    pub fn stream(&self, cfg: &Config) -> std::io::Result<Stream> {
        Stream::open(self, cfg)
    }

    /// Read contents of this file into memory as a [`String`].
    pub fn read_to_string(&self) -> Result<String, io::Error> {
        fs::read_to_string(&self.data.path)
    }

    pub fn open(&self) -> Result<impl Read, io::Error> {
        std::fs::File::open(&self.data.path)
    }

    pub fn entity_tag(&self) -> EntityTag<'static> {
        // Base64 encoding only uses bytes allowed in entity tags
        EntityTag::strong(base64::encode(&self.hash)).unwrap()
    }
}

impl std::ops::Deref for File {
    type Target = db::File;

    fn deref(&self) -> &db::File {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum FindFileError {
    /// Creation failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// File not found.
    #[fail(display = "No such file")]
    #[api(code = "file:not-found", status = "NOT_FOUND")]
    NotFound,
}

#[derive(ApiError, Debug, Fail, From)]
pub enum CreateFileError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// Obtaining connection from a pool of database connections.
    #[fail(display = "Pooling database connection: {}", _0)]
    #[api(internal)]
    DbPool(#[cause] #[from] r2d2::Error),
    /// System error.
    #[fail(display = "System error: {}", _0)]
    #[api(internal)]
    System(#[cause] #[from] io::Error),
}

impl From<tempfile::PersistError> for CreateFileError {
    fn from(e: tempfile::PersistError) -> Self {
        CreateFileError::System(e.error)
    }
}

/// Write stream into a sink and return hash of its contents.
fn copy_hash<S, C, W, E>(nn: usize, input: S, output: W)
    -> impl Future<Item=(Blake2bResult, W), Error=E>
where
    S: futures::Stream<Item=C>,
    C: AsRef<[u8]>,
    W: Write,
    E: From<S::Error>,
    E: From<io::Error>,
{
    let digest = Blake2b::new(nn);

    input
        .map_err(E::from)
        .fold((digest, output), |(mut digest, mut output), chunk| {
            digest.update(chunk.as_ref());

            match output.write_all(chunk.as_ref()) {
                Ok(_) => future::ok((digest, output)),
                Err(e) => future::err(E::from(e)),
            }
        })
        .map(|(digest, output)| (
            digest.finalize(),
            output,
        ))
}

struct HashWriter<W> {
    inner: W,
    digest: Blake2b,
}

impl<W> HashWriter<W> {
    fn new(nn: usize, inner: W) -> Self {
        HashWriter {
            inner,
            digest: Blake2b::new(nn),
        }
    }

    fn finalize(self) -> Blake2bResult {
        self.digest.finalize()
    }
}

impl<W> Write for HashWriter<W>
where
    W: Write,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.digest.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub struct Stream {
    stream: NamedFile,
    hash: Vec<u8>,
}

impl Stream {
    fn open(file: &File, cfg: &Config) -> std::io::Result<Stream> {
        let hash = bytes_to_hex(&file.data.hash);
        let path = cfg.storage.path.join(hash);
        let mime = file.data.mime.parse().expect("invalid mime type in database");
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
    type Item = HttpResponse;
    type Error = <NamedFile as Responder>::Error;

    fn respond_to<S: 'static>(self, req: &HttpRequest<S>)
    -> Result<Self::Item, Self::Error> {
        let mut rsp = self.stream.respond_to(req)?;

        let etag = format!(r#""{}""#, base64::encode(&self.hash));
        rsp.headers_mut().insert(ETAG, etag.try_into().unwrap());

        Ok(rsp)
    }
}
