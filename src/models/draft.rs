use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};
use failure::Fail;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    ApiError,
    db::{
        Connection,
        models as db,
        schema::{book_parts, documents, document_files, drafts, modules},
    },
};
use super::{
    File,
    document::{Document, PublicData as DocumentData},
};

#[derive(Debug)]
pub struct Draft {
    data: db::Draft,
    document: Document,
}

#[derive(Debug, Serialize)]
pub struct PublicData {
    pub module: Uuid,
    #[serde(flatten)]
    pub document: DocumentData,
}

impl Draft {
    /// Construct `Draft` from its database counterpart.
    pub(super) fn from_db(data: db::Draft, document: Document) -> Draft {
        Draft { data, document }
    }

    /// Get all drafts belonging to a user.
    pub fn all_of(dbconn: &Connection, user: i32) -> Result<Vec<Draft>, DbError> {
        drafts::table
            .filter(drafts::user.eq(user))
            .inner_join(documents::table)
            .get_results::<(db::Draft, db::Document)>(dbconn)
            .map(|v| {
                v.into_iter()
                    .map(|(data, document)| Draft {
                        data,
                        document: Document::from_db(document),
                    })
                    .collect()
            })
    }

    /// Find draft by ID.
    pub fn by_id(dbconn: &Connection, module: Uuid, user: i32)
    -> Result<Draft, FindDraftError> {
        drafts::table
            .filter(drafts::module.eq(module)
                .and(drafts::user.eq(user)))
            .inner_join(documents::table)
            .get_result::<(db::Draft, db::Document)>(dbconn)
            .optional()?
            .ok_or(FindDraftError::NotFound)
            .map(|(data, document)| Draft {
                data,
                document: Document::from_db(document),
            })
    }

    /// Delete this draft.
    pub fn delete(self, dbconn: &Connection) -> Result<(), DbError> {
        dbconn.transaction(|| {
            diesel::delete(&self.data).execute(dbconn)?;
            self.document.delete(dbconn)?;
            Ok(())
        })
    }

    /// Save this draft creating new version of the module from which it was
    /// created.
    pub fn save(self, dbconn: &Connection) -> Result<(), DbError> {
        dbconn.transaction(|| {
            diesel::update(
                modules::table
                    .filter(modules::id.eq(self.data.module)))
                .set(modules::document.eq(self.data.document))
                .execute(dbconn)?;

            diesel::delete(&self.data)
                .execute(dbconn)?;

            Ok(())
        })
    }

    /// Get the public portion of this drafts's data.
    pub fn get_public(&self) -> PublicData {
        PublicData {
            module: self.data.module,
            document: self.document.get_public(),
        }
    }

    /// Query list of books containing module this draft was derived from.
    pub fn get_books(&self, dbconn: &Connection) -> Result<Vec<Uuid>, DbError> {
        Ok(book_parts::table
            .filter(book_parts::module.eq(self.data.module))
            .get_results::<db::BookPart>(dbconn)?
            .into_iter()
            .map(|part| part.book)
            .collect())
    }

    /// Write into a file in this draft.
    ///
    /// If there already is a file with this name it will be updated, otherwise
    /// a new file will be created.
    pub fn write_file(&self, dbconn: &Connection, name: &str, file: &File)
    -> Result<(), DbError> {
        if name == "index.cnxml" {
            diesel::update(&*self.document)
                .set(documents::index.eq(file.id))
                .execute(dbconn)?;
            return Ok(());
        }

        diesel::insert_into(document_files::table)
            .values(&db::NewDocumentFile {
                document: self.document.id,
                name,
                file: file.id,
            })
            .on_conflict((document_files::document, document_files::name))
            .do_update()
            .set(document_files::file.eq(file.id))
            .execute(dbconn)?;
        Ok(())
    }

    /// Delete a file from this draft.
    pub fn delete_file(&self, dbconn: &Connection, name: &str) -> Result<(), DbError> {
        diesel::delete(document_files::table
            .filter(document_files::document.eq(self.document.id)
                .and(document_files::name.eq(name))))
            .execute(dbconn)?;
        Ok(())
    }

    /// Change title of this draft's document.
    pub fn set_title(&mut self, dbconn: &Connection, title: &str) -> Result<(), DbError> {
        self.document.set_title(dbconn, title)
    }
}

impl std::ops::Deref for Draft {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.document
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindDraftError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// No draft found matching given criteria.
    #[fail(display = "No such draft")]
    #[api(code = "draft:not-found", status = "NOT_FOUND")]
    NotFound,
}

impl_from! { for FindDraftError ;
    DbError => |e| FindDraftError::Database(e),
}
