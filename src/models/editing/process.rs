use diesel::{
    prelude::*,
    result::Error as DbError,
};
use std::ops::Deref;

use crate::db::{
    Connection,
    models as db,
    schema::edit_processes,
};

/// An editing process.
///
/// See [module description][super] for details.
#[derive(Clone, Debug)]
pub struct Process {
    data: db::EditProcess,
}

/// A subset of role's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub id: i32,
    pub name: String,
}

impl Process {
    /// Construct `Process` from its database counterpart.
    pub(super) fn from_db(data: db::EditProcess) -> Process {
        Process { data }
    }

    /// Get all modules.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all(dbcon: &Connection) -> Result<Vec<Process>, DbError> {
        edit_processes::table
            .get_results::<db::EditProcess>(dbcon)
            .map(|v| v.into_iter().map(Process::from_db).collect())
    }

    /// Find a process by ID.
    pub fn by_id(dbcon: &Connection, id: i32) -> Result<Process, FindProcessError> {
        edit_processes::table
            .filter(edit_processes::id.eq(id))
            .get_result::<db::EditProcess>(dbcon)
            .optional()?
            .ok_or(FindProcessError::NotFound)
            .map(Process::from_db)
    }

    /// Get public portion of this process's data.
    pub fn get_public(&self) -> PublicData {
        let db::EditProcess { id, ref name, .. } = self.data;

        PublicData {
            id,
            name: name.clone(),
        }
    }
}

impl Deref for Process {
    type Target = db::EditProcess;

    fn deref(&self) -> &db::EditProcess {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindProcessError {
    /// Database error.
    #[api(internal)]
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// No process found matching given criteria.
    #[api(code = "edit-process:not-found", status = "NOT_FOUND")]
    #[fail(display = "No such process")]
    NotFound,
}

impl_from! { for FindProcessError ;
    DbError => |e| FindProcessError::Database(e),
}
