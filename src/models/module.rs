use actix_web::{HttpResponse, ResponseError};
use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};
use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::{documents, modules},
};
use super::{Document, File};

/// A module is a version of Document that can be part of a Book.
#[derive(Debug)]
pub struct Module {
    data: db::Module,
    document: Document,
}

/// A subset of module's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub id: Uuid,
    pub name: String,
    pub assignee: Option<i32>,
}

impl Module {
    /// Get all modules.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all(dbconn: &Connection) -> Result<Vec<Module>, DbError> {
        modules::table
            .inner_join(documents::table)
            .get_results::<(db::Module, db::Document)>(dbconn)
            .map(|v| {
                v.into_iter()
                    .map(|(data, document)| Module {
                        data,
                        document: Document::from_db(document),
                    })
                    .collect()
            })
    }

    /// Find a module by ID.
    pub fn by_id(dbconn: &Connection, id: Uuid) -> Result<Module, FindModuleError> {
        modules::table
            .filter(modules::id.eq(id))
            .inner_join(documents::table)
            .get_result::<(db::Module, db::Document)>(dbconn)
            .optional()?
            .ok_or(FindModuleError::NotFound)
            .map(|(data, document)| Module {
                data,
                document: Document::from_db(document),
            })
    }

    /// Create a new module.
    pub fn create<'c, N, I>(
        dbconn: &Connection,
        title: &str,
        index: File,
        files: I,
    ) -> Result<Module, DbError>
    where
        I: IntoIterator<Item = &'c (N, File)>,
        N: AsRef<str> + 'c,
    {
        dbconn.transaction(|| {
            let document = Document::create(dbconn, title, index, files)?;

            let data = diesel::insert_into(modules::table)
                .values(&db::Module {
                    id: Uuid::new_v4(),
                    document: document.id,
                    assignee: None,
                })
                .get_result::<db::Module>(dbconn)?;

            Ok(Module { data, document })
        })
    }

    /// Get ID of this module.
    ///
    /// Since `Module` derefs to [`Document`], `module.id` will return ID of the
    /// this module's current document.
    pub fn id(&self) -> Uuid {
        self.data.id
    }

    /// Get the public portion of this module's data.
    pub fn get_public(&self) -> PublicData {
        PublicData {
            id: self.data.id,
            name: self.document.name.clone(),
            assignee: self.data.assignee,
        }
    }
}

impl std::ops::Deref for Module {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.document
    }
}

#[derive(Debug, Fail)]
pub enum FindModuleError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// No module found matching given criteria.
    #[fail(display = "No such module")]
    NotFound,
}

impl_from! { for FindModuleError ;
    DbError => |e| FindModuleError::Database(e),
}

impl ResponseError for FindModuleError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            FindModuleError::Database(_) =>
                HttpResponse::InternalServerError().finish(),
            FindModuleError::NotFound =>
                HttpResponse::NotFound().finish(),
        }
    }
}
