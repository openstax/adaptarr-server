use diesel::{
    prelude::*,
    result::Error as DbError,
};
use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::{documents, drafts},
};
use super::Document;

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

    /// Get the public portion of this drafts's data.
    pub fn get_public(&self) -> PublicData {
        PublicData {
            module: self.data.module,
            name: self.document.name.clone(),
        }
    }
}

impl std::ops::Deref for Draft {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.document
    }
}
