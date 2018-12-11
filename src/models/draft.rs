use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};
use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::{documents, document_files, drafts},
};
use super::{Document, File};

#[derive(Debug)]
pub struct Draft {
    data: db::Draft,
    document: Document,
}

#[derive(Debug, Serialize)]
pub struct PublicData {
    pub module: Uuid,
    pub name: String,
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
    pub fn by_id(dbconn: &Connection, module: Uuid, user: i32) -> Result<Draft, DbError> {
        drafts::table
            .filter(drafts::module.eq(module)
                .and(drafts::user.eq(user)))
            .inner_join(documents::table)
            .get_result::<(db::Draft, db::Document)>(dbconn)
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

    /// Get the public portion of this drafts's data.
    pub fn get_public(&self) -> PublicData {
        PublicData {
            module: self.data.module,
            name: self.document.name.clone(),
        }
    }

    /// Write into a file in this draft.
    ///
    /// If there already is a file with this name it will be updated, otherwise
    /// a new file will be created.
    pub fn write_file(&self, dbconn: &Connection, name: &str, file: &File)
    -> Result<(), DbError> {
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
}

impl std::ops::Deref for Draft {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.document
    }
}
