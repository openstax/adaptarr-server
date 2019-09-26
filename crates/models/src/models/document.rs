use diesel::{Connection as _, prelude::*, result::Error as DbError};
use serde::Serialize;

use crate::{
    audit,
    db::{Connection, models as db, schema::{documents, document_files, files}},
};
use super::{File, FindModelResult, Model};

/// Document model serves as a shared foundation for modules and drafts. You
/// don't construct a `Document`, but obtain it via [`std::ops::Deref`] from
/// modules or drafts.
#[derive(Debug)]
pub struct Document {
    data: db::Document,
}

/// A subset of document's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    pub title: String,
    pub language: String,
}

impl Model for Document {
    const ERROR_CATEGORY: &'static str = "document";

    type Id = i32;
    type Database = db::Document;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, id: i32) -> FindModelResult<Document> {
        documents::table
            .filter(documents::id.eq(id))
            .get_result(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Document { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> i32 {
        self.data.id
    }

    fn get_public(&self) -> Public {
        Public {
            title: self.data.title.clone(),
            language: self.data.language.clone(),
        }
    }
}

impl Document {
    /// Create a new document.
    pub(super) fn create<N, I>(
        db: &Connection,
        title: &str,
        language: &str,
        index: File,
        files: I,
    )  -> Result<Document, DbError>
    where
        I: IntoIterator<Item = (N, File)>,
        N: AsRef<str>,
    {
        db.transaction(|| {
            let data = diesel::insert_into(documents::table)
                .values(&db::NewDocument {
                    title,
                    language,
                    index: index.id,
                })
                .get_result::<db::Document>(db)?;

            let mut new_files = Vec::new();

            for (name, file) in files {
                let file = diesel::insert_into(document_files::table)
                    .values(&db::NewDocumentFile {
                        document: data.id,
                        name: name.as_ref(),
                        file: file.id,
                    })
                    .get_result::<db::DocumentFile>(db)?;

                new_files.push(LogNewFile {
                    name: file.name,
                    file: file.file,
                });
            }

            audit::log_db(db, "documents", data.id, "create", LogNewDocument {
                title,
                language,
                index: index.id,
                files: new_files,
            });

            Ok(Document { data })
        })
    }

    /// Delete this document.
    pub(super) fn delete(self, db: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(db)?;
        Ok(())
    }

    /// Get list of files in this document.
    pub fn get_files(&self, db: &Connection) -> Result<Vec<(String, File)>, DbError> {
        document_files::table
            .filter(document_files::document.eq(self.data.id))
            .inner_join(files::table)
            .get_results::<(db::DocumentFile, db::File)>(db)
            .map(|r| r.into_iter()
                .map(|(d, f)| (d.name, File::from_db(f)))
                .collect())
    }

    /// Get a file from this document.
    pub fn get_file(&self, db: &Connection, name: &str)
    -> FindModelResult<File> {
        if name == "index.cnxml" {
            return File::by_id(db, self.data.index);
        }

        document_files::table
            .filter(document_files::document.eq(self.data.id)
                .and(document_files::name.eq(name)))
            .inner_join(files::table)
            .get_result::<(db::DocumentFile, db::File)>(db)
            .map(|(_, data)| File::from_db(data))
            .map_err(From::from)
    }

    /// Set this document's title.
    pub(super) fn set_title(&mut self, db: &Connection, title: &str)
    -> Result<(), DbError> {
        diesel::update(&self.data)
            .set(documents::title.eq(title))
            .execute(db)?;

        self.data.title = title.to_string();

        Ok(())
    }
}

impl std::ops::Deref for Document {
    type Target = db::Document;

    fn deref(&self) -> &db::Document {
        &self.data
    }
}

#[derive(Serialize)]
struct LogNewDocument<'a> {
    title: &'a str,
    language: &'a str,
    index: i32,
    files: Vec<LogNewFile>,
}

#[derive(Serialize)]
struct LogNewFile {
    name: String,
    file: i32,
}
