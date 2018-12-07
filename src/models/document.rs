use diesel::{
    prelude::*,
    result::Error as DbError,
};

use crate::db::{
    Connection,
    models as db,
    schema::document_files,
};

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

    /// Get list of files in this document.
    pub fn get_files(&self, dbconn: &Connection) -> Result<Vec<String>, DbError> {
        document_files::table
            .filter(document_files::document.eq(self.data.id))
            .get_results::<db::DocumentFile>(dbconn)
            .map(|r| r.into_iter().map(|f| f.name).collect())
    }
}

impl std::ops::Deref for Document {
    type Target = db::Document;

    fn deref(&self) -> &db::Document {
        &self.data
    }
}
