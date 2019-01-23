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
    pub(super) fn create<N, I>(
        dbconn: &Connection,
        title: &str,
        index: File,
        files: I,
    )  -> Result<Document, DbError>
    where
        I: IntoIterator<Item = (N, File)>,
        N: AsRef<str>,
    {
        dbconn.transaction(|| {
            let data = diesel::insert_into(documents::table)
                .values(&db::NewDocument {
                    title,
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
    pub fn get_files(&self, dbconn: &Connection) -> Result<Vec<(String, File)>, DbError> {
        document_files::table
            .filter(document_files::document.eq(self.data.id))
            .inner_join(files::table)
            .get_results::<(db::DocumentFile, db::File)>(dbconn)
            .map(|r| r.into_iter()
                .map(|(d, f)| (d.name, File::from_db(f)))
                .collect())
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

#[derive(ApiError, Debug, Fail)]
pub enum GetFileError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// No such file.
    #[fail(display = "No such file")]
    #[api(code = "file:not-found", status = "NOT_FOUND")]
    NotFound,
}

impl_from! { for GetFileError ;
    DbError => |e| GetFileError::Database(e),
}
