use actix_web::{HttpResponse, ResponseError};
use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};

use crate::db::{
    Connection,
    models as db,
    schema::{documents, document_files, files},
};
use super::file::{File, FindFileError};

/// Document model serves as a shared foundation for modules and drafts. You
/// don't construct a `Document`, but obtain it via [`std::ops::Deref`] from
/// modules or drafts.
#[derive(Debug)]
pub struct Document {
    data: db::Document,
}

impl Document {
    /// Construct `Document` from its database counterpart.
    pub(super) fn from_db(data: db::Document) -> Document {
        Document { data }
    }

    /// Create a new document.
    pub(super) fn create<'c, N, I>(
        dbconn: &Connection,
        name: &str,
        index: File,
        files: I,
    )  -> Result<Document, DbError>
    where
        I: IntoIterator<Item = &'c (N, File)>,
        N: AsRef<str> + 'c,
    {
        dbconn.transaction(|| {
            let data = diesel::insert_into(documents::table)
                .values(&db::NewDocument {
                    name,
                    index: index.id,
                })
                .get_result::<db::Document>(dbconn)?;

            for (name, file) in files {
                diesel::insert_into(document_files::table)
                    .values(&db::NewDocumentFile {
                        document: data.id,
                        name: name.as_ref(),
                        file: file.id,
                    })
                    .execute(dbconn)?;
            }

            Ok(Document { data })
        })
    }

    /// Delete this document.
    pub(super) fn delete(self, dbconn: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(dbconn)?;
        Ok(())
    }

    /// Get list of files in this document.
    pub fn get_files(&self, dbconn: &Connection) -> Result<Vec<String>, DbError> {
        document_files::table
            .filter(document_files::document.eq(self.data.id))
            .get_results::<db::DocumentFile>(dbconn)
            .map(|r| r.into_iter().map(|f| f.name).collect())
    }

    /// Get a file from this document.
    pub fn get_file(&self, dbconn: &Connection, name: &str)
    -> Result<File, GetFileError> {
        if name == "index.cnxml" {
            return File::by_id(dbconn, self.data.index)
                .map_err(|e| match e {
                    FindFileError::Database(e) => GetFileError::Database(e),
                    FindFileError::NotFound => panic!(
                        "Inconsistent database: missing file for index.cnxml"),
                });
        }

        document_files::table
            .filter(document_files::document.eq(self.data.id)
                .and(document_files::name.eq(name)))
            .inner_join(files::table)
            .get_result::<(db::DocumentFile, db::File)>(dbconn)
            .optional()?
            .ok_or(GetFileError::NotFound)
            .map(|(_, data)| File::from_db(data))
    }
}

impl std::ops::Deref for Document {
    type Target = db::Document;

    fn deref(&self) -> &db::Document {
        &self.data
    }
}

#[derive(Debug, Fail)]
pub enum GetFileError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// No such file.
    #[fail(display = "No such file")]
    NotFound,
}

impl_from! { for GetFileError ;
    DbError => |e| GetFileError::Database(e),
}

impl ResponseError for GetFileError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            GetFileError::Database(_) =>
                HttpResponse::InternalServerError().finish(),
            GetFileError::NotFound =>
                HttpResponse::NotFound().finish(),
        }
    }
}
