use diesel::{
    Connection as _,
    prelude::*,
    result::Error as DbError,
};
use failure::Fail;
use serde::Serialize;
use std::ops::Deref;

use crate::{
    ApiError,
    db::{
        Connection,
        models as db,
        schema::{
            edit_process_versions,
            edit_processes,
        },
    },
};
use super::{structure, version::{Version, CreateVersionError}};

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

    /// Create a new editing process.
    pub fn create(dbcon: &Connection, structure: &structure::Process)
    -> Result<Version, CreateVersionError> {
        dbcon.transaction(|| {
            let process = diesel::insert_into(edit_processes::table)
                .values(&db::NewEditProcess {
                    name: &structure.name,
                })
                .get_result::<db::EditProcess>(dbcon)?;

            Version::create(dbcon, Self::from_db(process), structure)
        })
    }

    pub fn into_db(self) -> db::EditProcess {
        self.data
    }

    /// Delete this editing process.
    ///
    /// Note that only processes which have never been used can be deleted.
    pub fn delete(self, dbcon: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(dbcon)?;
        Ok(())
    }

    /// Get public portion of this process's data.
    pub fn get_public(&self) -> PublicData {
        let db::EditProcess { id, ref name, .. } = self.data;

        PublicData {
            id,
            name: name.clone(),
        }
    }

    /// Get list of all versions of this process.
    pub fn get_versions(&self, dbcon: &Connection)
    -> Result<Vec<Version>, DbError> {
        Ok(edit_process_versions::table
            .filter(edit_process_versions::process.eq(self.data.id))
            .order_by(edit_process_versions::version.desc())
            .get_results::<db::EditProcessVersion>(dbcon)?
            .into_iter()
            .map(|version| Version::from_db(version, self.clone()))
            .collect())
    }

    /// Get current (latest) version of this process.
    pub fn get_current(&self, dbcon: &Connection) -> Result<Version, DbError> {
        edit_process_versions::table
            .filter(edit_process_versions::process.eq(self.data.id))
            .order_by(edit_process_versions::version.desc())
            .limit(1)
            .get_result::<db::EditProcessVersion>(dbcon)
            .map(|version| Version::from_db(version, self.clone()))
    }

    /// Set process's name.
    pub fn set_name(&mut self, dbcon: &Connection, name: &str)
    -> Result<(), DbError> {
        self.data = diesel::update(&self.data)
            .set(edit_processes::name.eq(name))
            .get_result(dbcon)?;
        Ok(())
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
