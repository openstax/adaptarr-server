use actix_web::{HttpResponse, ResponseError};
use diesel::{
    prelude::*,
    result::Error as DbError,
};
use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::{documents, modules},
};

/// A module is a version of Document that can be part of a Book.
#[derive(Debug)]
pub struct Module {
    data: db::Module,
    document: db::Document,
}

/// A subset of module's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub id: Uuid,
    pub name: String,
    pub assignee: Option<i32>,
}

impl Module {
    /// Find a module by ID.
    pub fn by_id(dbconn: &Connection, id: Uuid) -> Result<Module, FindModuleError> {
        modules::table
            .filter(modules::id.eq(id))
            .inner_join(documents::table)
            .get_result::<(db::Module, db::Document)>(dbconn)
            .optional()?
            .ok_or(FindModuleError::NotFound)
            .map(|(data, document)| Module { data, document })
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
