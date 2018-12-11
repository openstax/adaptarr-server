use actix_web::{HttpResponse, Responder, ResponseError, fs::NamedFile};
use blake2::blake2b::{Blake2b, Blake2bResult};
use diesel::{
    prelude::*,
    result::Error as DbError,
};
use futures::{Future, Stream, future};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};
use tempfile::{Builder as TempBuilder, NamedTempFile};

use crate::{
    Config,
    db::{
        Connection,
        Pool,
        models as db,
        schema::files,
    },
};

thread_local! {
    static MAGIC: magic::Cookie = {
        let cookie = magic::Cookie::open(magic::flags::MIME)
            .expect("libmagic to initialize");
        cookie.load(&["/usr/share/misc/magic"])
            .expect("libmagic to load database");
        cookie
    };
}

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
    pub fn from_stream<S, I, E>(dbpool: Pool, storage: PathBuf, data: S)
        -> impl Future<Item=File, Error=E>
    where
        S: Stream<Item=I>,
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
                        File::from_file_with_hash(&*db, storage, tmp, hash))
                    .map_err(E::from)))
    }

    /// Create new file from a data in memory.
    pub fn from_data<B>(dbconn: &Connection, config: &Config, data: B)
        -> Result<File, CreateFileError>
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
                let name = hash_to_hex(hash.as_bytes());
                let path = config.storage.path.join(name);

                let mut file = std::fs::File::create(&path)?;
                file.write_all(data.as_ref())?;

                let mime = MAGIC.with(|magic| magic.file(&path))
                    .expect("libmagic to work");

                diesel::insert_into(files::table)
                    .values(db::NewFile {
                        mime: &mime,
                        path: path.to_str().expect("invalid path"),
                        hash: hash.as_bytes(),
                    })
                    .get_result::<db::File>(dbconn)
                    .map_err(Into::into)
                    .map(|data| File { data })
            },
        }
    }

    /// Create new file from a temporary file and hash.
    ///
    /// This is an internal constructor.
    fn from_file_with_hash<P>(
        dbconn: &Connection,
        storage: P,
        file: NamedTempFile,
        hash: Blake2bResult,
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
                let name = hash_to_hex(hash.as_bytes());
                let path = storage.as_ref().join(name);
                let _ = file.persist(&path)?;

                let mime = MAGIC.with(|magic| magic.file(&path))
                    .expect("libmagic to work");

                diesel::insert_into(files::table)
                    .values(db::NewFile {
                        mime: &mime,
                        path: path.to_str().expect("invalid path"),
                        hash: hash.as_bytes(),
                    })
                    .get_result::<db::File>(dbconn)
                    .map_err(Into::into)
                    .map(|data| File { data })
            }
        }
    }

    /// Get an Actix responder streaming contents of this file.
    pub fn stream(&self, cfg: &Config) -> impl Responder {
        let hash = hash_to_hex(&self.data.hash);
        let path = cfg.storage.path.join(hash);
        NamedFile::open(path)
    }
}

impl std::ops::Deref for File {
    type Target = db::File;

    fn deref(&self) -> &db::File {
        &self.data
    }
}

#[derive(Debug, Fail)]
pub enum FindFileError {
    /// Creation failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// File not found.
    #[fail(display = "No such file")]
    NotFound,
}

impl_from! { for FindFileError ;
    DbError => |e| FindFileError::Database(e),
}

#[derive(Debug, Fail)]
pub enum CreateFileError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// Obtaining connection from a pool of database connections.
    #[fail(display = "Pooling database connection: {}", _0)]
    DbPool(#[cause] r2d2::Error),
    /// System error.
    #[fail(display = "System error: {}", _0)]
    System(#[cause] io::Error),
}

impl_from! { for CreateFileError ;
    DbError => |e| CreateFileError::Database(e),
    r2d2::Error => |e| CreateFileError::DbPool(e),
    io::Error => |e| CreateFileError::System(e),
    tempfile::PersistError => |e| CreateFileError::System(e.error),
}

impl ResponseError for CreateFileError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::InternalServerError().finish()
    }
}

/// Write stream into a sing and return hash of its contents.
fn copy_hash<S, C, W, E>(nn: usize, input: S, output: W)
    -> impl Future<Item=(Blake2bResult, W), Error=E>
where
    S: Stream<Item=C>,
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

fn hash_to_hex(hash: &[u8]) -> String {
    use std::fmt::Write;

    let mut hex = String::with_capacity(hash.len() * 4);

    for byte in hash {
        write!(hex, "{:02x}", byte).unwrap();
    }

    hex
}
