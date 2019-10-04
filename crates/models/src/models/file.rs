use adaptarr_error::ApiError;
use adaptarr_macros::From;
use adaptarr_util::bytes_to_hex;
use blake2::blake2b::{Blake2b, Blake2bResult};
use diesel::{prelude::*, result::Error as DbError};
use failure::Fail;
use futures::{Future, Stream as _, future};
use std::{
    convert::Infallible,
    fs,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};
use tempfile::{Builder as TempBuilder, NamedTempFile};

use crate::db::{Connection, Pool, models as db, schema::files};
use super::{FindModelResult, Model};

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

impl Model for File {
    const ERROR_CATEGORY: &'static str = "file";

    type Id = i32;
    type Database = db::File;
    type Public = Infallible;
    type PublicParams = Infallible;

    fn by_id(db: &Connection, id: i32) -> FindModelResult<File> {
        files::table
            .filter(files::id.eq(id))
            .get_result::<db::File>(db)
            .map(File::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        File { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> i32 {
        self.data.id
    }

    fn get_public(&self) -> Infallible {
        unreachable!()
    }
}

impl File {
    /// Create new file from a stream of bytes.
    pub fn from_stream<P, S, I, E>(
        dbpool: Pool,
        storage_path: P,
        data: S,
        mime: Option<&'static str>,
    ) -> impl Future<Item=File, Error=E>
    where
        P: AsRef<Path>,
        S: futures::Stream<Item=I>,
        I: AsRef<[u8]>,
        E: From<CreateFileError> + From<S::Error> + From<io::Error>,
    {
        future::result(TempBuilder::new().tempfile_in(storage_path.as_ref()))
            .map_err(E::from)
            .and_then(|tmp| copy_hash(64, data, tmp))
            .and_then(move |(hash, tmp)| future::result(
                dbpool.get()
                    .map_err(Into::into)
                    .and_then(|db|
                        File::from_file_with_hash(&*db, storage_path.as_ref(), tmp, hash, mime))
                    .map_err(E::from)))
    }

    /// Create new file from a data in memory.
    pub fn from_data<P, B>(
        db: &Connection,
        storage_path: P,
        data: B,
        mime: Option<&str>,
    ) -> Result<File, CreateFileError>
    where
        P: AsRef<Path>,
        B: AsRef<[u8]>,
    {
        let mut hash = Blake2b::new(64);
        hash.update(data.as_ref());
        let hash = hash.finalize();

        match files::table
            .filter(files::hash.eq(hash.as_bytes()))
            .get_result::<db::File>(db)
            .optional()?
        {
            // There already is a file with this hash.
            Some(data) => Ok(File { data }),
            // It's a new file; we need to create database entry for it.
            None => {
                let name = bytes_to_hex(hash.as_bytes());
                let path = storage_path.as_ref().join(name);

                let mut file = std::fs::File::create(&path)?;
                file.write_all(data.as_ref())?;

                let magic = match mime {
                    Some(_) => None,
                    None => Some(MAGIC.with(|magic| magic.file(&path))
                        .expect("libmagic to work")),
                };

                let mime = mime.or_else(|| magic.as_ref().map(String::as_str)).unwrap();

                diesel::insert_into(files::table)
                    .values(db::NewFile {
                        mime,
                        path: path.to_str().expect("invalid path"),
                        hash: hash.as_bytes(),
                    })
                    .get_result::<db::File>(db)
                    .map_err(Into::into)
                    .map(|data| File { data })
            },
        }
    }

    /// Create new file from any type implementing [`std::io::Write`].
    pub fn from_read<P, R>(
        db: &Connection,
        storage_path: P,
        mut read: R,
        mime: Option<&str>,
    ) -> Result<File, CreateFileError>
    where
        P: AsRef<Path>,
        R: Read,
    {
        let mut tmp = NamedTempFile::new_in(storage_path.as_ref())?;

        let digest = {
            let mut hash = HashingWriter::new(64, &mut tmp);
            io::copy(&mut read, &mut hash)?;
            hash.finalize()
        };

        File::from_file_with_hash(db, storage_path.as_ref(), tmp, digest, mime)
    }

    /// Create a new file from a temporary file.
    pub fn from_temporary<P>(
        db: &Connection,
        storage_path: P,
        mut file: NamedTempFile,
        mime: Option<&str>,
    ) -> Result<File, CreateFileError>
    where
        P: AsRef<Path>,
    {
        let digest = {
            let mut sink = io::sink();
            let mut hash = HashingWriter::new(64, &mut sink);
            file.seek(SeekFrom::Start(0))?;
            io::copy(&mut file, &mut hash)?;
            hash.finalize()
        };

        File::from_file_with_hash(db, storage_path, file, digest, mime)
    }

    /// Create new file from a temporary file and hash.
    ///
    /// This is an internal constructor.
    fn from_file_with_hash<P>(
        db: &Connection,
        storage_path: P,
        file: NamedTempFile,
        hash: Blake2bResult,
        mime: Option<&str>,
    ) -> Result<File, CreateFileError>
    where
        P: AsRef<Path>,
    {
        match files::table
            .filter(files::hash.eq(hash.as_bytes()))
            .get_result::<db::File>(db)
            .optional()?
        {
            // There already is a file with this hash.
            Some(data) => Ok(File { data }),
            // It's a new file; we need to create database entry for it.
            None => {
                let name = bytes_to_hex(hash.as_bytes());
                let path = storage_path.as_ref().join(name);
                let _ = file.persist(&path)?;

                let magic = match mime {
                    Some(_) => None,
                    None => Some(MAGIC.with(|magic| magic.file(&path))
                        .expect("libmagic to work")),
                };

                let mime = mime.or_else(|| magic.as_ref().map(String::as_str)).unwrap();

                diesel::insert_into(files::table)
                    .values(db::NewFile {
                        mime,
                        path: path.to_str().expect("invalid path"),
                        hash: hash.as_bytes(),
                    })
                    .get_result::<db::File>(db)
                    .map_err(Into::into)
                    .map(|data| File { data })
            }
        }
    }

    /// Read contents of this file into memory as a [`String`].
    pub fn read_to_string(&self) -> Result<String, io::Error> {
        fs::read_to_string(&self.data.path)
    }

    pub fn open(&self) -> Result<impl Read, io::Error> {
        std::fs::File::open(&self.data.path)
    }
}

impl std::ops::Deref for File {
    type Target = db::File;

    fn deref(&self) -> &db::File {
        &self.data
    }
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
    E: From<S::Error> + From<io::Error>,
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

/// Wrapper around an [`std::io::Write`] which hashes contents written to it.
struct HashingWriter<W> {
    inner: W,
    digest: Blake2b,
}

impl<W> HashingWriter<W> {
    fn new(nn: usize, inner: W) -> Self {
        HashingWriter {
            inner,
            digest: Blake2b::new(nn),
        }
    }

    fn finalize(self) -> Blake2bResult {
        self.digest.finalize()
    }
}

impl<W> Write for HashingWriter<W>
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
