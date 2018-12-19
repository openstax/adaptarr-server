//! File upload and importing ZIPs of modules and collections.

use actix::{Actor, Addr, Handler, SyncArbiter, SyncContext, Message};
use actix_web::{HttpResponse, ResponseError};
use tempfile::NamedTempFile;
use zip::{ZipArchive, result::ZipError};
use std::path::PathBuf;
use diesel::result::Error as DbError;

use crate::{
    config::Storage,
    db::Pool,
    models::{
        module::{Module, ReplaceModuleError},
        file::{File, CreateFileError},
    },
};

/// Request a new module to be created from contents of a ZIP file
pub struct ImportModule {
    pub title: String,
    pub file: NamedTempFile,
}

impl Message for ImportModule {
    type Result = Result<Module, ImportError>;
}

/// Requests contents of an existing module to be replaced with contents of
/// a ZIP file.
pub struct ReplaceModule {
    pub module: Module,
    pub file: NamedTempFile,
}

impl Message for ReplaceModule {
    type Result = Result<Module, ImportError>;
}

/// Actix actor processing ZIPs in a background worker.
pub struct Importer {
    pool: Pool,
    config: Storage,
}

impl Importer {
    pub fn new(pool: Pool, config: Storage) -> Importer {
        Importer {
            pool,
            config,
        }
    }

    pub fn start(pool: Pool, config: Storage) -> Addr<Importer> {
        SyncArbiter::start(1, move || Importer::new(pool.clone(), config.clone()))
    }

    /// Process a zipped module and extract index.cnxml and other media files
    /// from it.
    fn process_module_zip(&mut self, mut file: NamedTempFile)
    -> Result<(File, Vec<(String, File)>), ImportError> {
        let mut zip = ZipArchive::new(file.as_file_mut())?;

        // NOTE: Looking for index imperatively because rustc doesn't seem able
        // to infer types for iterator combinators (“consider giving this
        // closure parameter a type” on a parameter that already has a type).
        let mut index = None;

        for inx in 0..zip.len() {
            let file = zip.by_index(inx)?;
            let path = file.sanitized_name();
            let name = path
                .file_name()
                .unwrap();

            if name == "index.cnxml" {
                index = Some((
                    inx,
                    path.parent().map_or_else(|| PathBuf::new(), |p| p.to_owned()),
                ));
                break;
            }
        }

        let (index, base_path) = index.ok_or(ImportError::IndexMissing)?;

        let db = self.pool.get()?;

        let index_file = File::from_read(
            &*db,
            &self.config,
            zip.by_index(index)?,
        )?;

        let mut files = Vec::new();

        for inx in 0..zip.len() {
            // Don't import index twice.
            if inx == index {
                continue;
            }

            let file = zip.by_index(inx)?;

            // Don't import directories.
            if file.size() == 0 {
                continue;
            }

            let path = file.sanitized_name();

            if !path.starts_with(&base_path) {
                continue;
            }

            let name = path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();

            let file = File::from_read(&*db, &self.config, file)?;

            files.push((name, file));
        }

        Ok((index_file, files))
    }

    /// Create a new module from a ZIP of its contents.
    fn create_module(&mut self, title: String, file: NamedTempFile)
    -> Result<Module, ImportError> {
        let (index, files) = self.process_module_zip(file)?;
        let db = self.pool.get()?;
        let module = Module::create(&*db, &title, index, files)?;

        Ok(module)
    }

    /// Import a zipped module onto an existing one.
    fn replace_module(&mut self, mut module: Module, file: NamedTempFile)
    -> Result<Module, ImportError> {
        let (index, files) = self.process_module_zip(file)?;

        let db = self.pool.get()?;
        module.replace(&*db, index, files)?;

        Ok(module)
    }
}

impl Actor for Importer {
    type Context = SyncContext<Self>;
}

impl Handler<ImportModule> for Importer {
    type Result = Result<Module, ImportError>;

    fn handle(&mut self, msg: ImportModule, _: &mut Self::Context) -> Self::Result {
        let ImportModule { title, file } = msg;

        self.create_module(title, file)
    }
}

impl Handler<ReplaceModule> for Importer {
    type Result = Result<Module, ImportError>;

    fn handle(&mut self, msg: ReplaceModule, _: &mut Self::Context) -> Self::Result {
        let ReplaceModule { module, file } = msg;

        self.replace_module(module, file)
    }
}

#[derive(Debug, Fail)]
pub enum ImportError {
    /// There was a problem with the ZIP archive.
    #[fail(display = "{}", _0)]
    Archive(#[cause] ZipError),
    /// There was no file named index.cnxml in the ZIP archive.
    #[fail(display = "Archive is missing index.cnxml")]
    IndexMissing,
    /// There was a problem obtaining database connection.
    #[fail(display = "Cannot obtain database connection: {}", _0)]
    DbPool(#[cause] r2d2::Error),
    /// A file could not be created.
    #[fail(display = "Cannot create file: {}", _0)]
    FileCreation(#[cause] CreateFileError),
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// Replacing module's contents failed.
    #[fail(display = "{}", _0)]
    ReplaceModule(#[cause] ReplaceModuleError),
}

impl_from! { for ImportError ;
    r2d2::Error => |e| ImportError::DbPool(e),
    ZipError => |e| ImportError::Archive(e),
    CreateFileError => |e| ImportError::FileCreation(e),
    DbError => |e| ImportError::Database(e),
    ReplaceModuleError => |e| ImportError::ReplaceModule(e),
}

impl ResponseError for ImportError {
    fn error_response(&self) -> HttpResponse {
        use self::ImportError::*;

        match *self {
            Archive(_) | IndexMissing => HttpResponse::BadRequest()
                .body(self.to_string()),
            DbPool(_) | Database(_) => HttpResponse::InternalServerError()
                .finish(),
            FileCreation(ref e) => e.error_response(),
            ReplaceModule(ref e) => e.error_response(),
        }
    }
}
