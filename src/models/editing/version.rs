use chrono::NaiveDateTime;
use diesel::{
    prelude::*,
    result::Error as DbError,
};
use std::ops::Deref;

use crate::db::{
    Connection,
    models as db,
    schema::{
        edit_process_versions,
        edit_processes,
    },
};
use super::Process;

/// Particular revision of an editing [`Process`][Process]
///
/// See [module description][super] for details.
///
/// [Process]: ../process/struct.Process.html
#[derive(Debug)]
pub struct Version {
    data: db::EditProcessVersion,
    process: Process,
}

/// A subset of version's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub id: i32,
    pub name: String,
    pub version: NaiveDateTime,
}

impl Version {
    /// Construct `Version` from its database counterpart.
    pub(super) fn from_db(
        data: db::EditProcessVersion,
        process: Process,
    ) -> Version {
        Version { data, process }
    }

    /// Find a version by ID.
    pub fn by_id(dbcon: &Connection, process: i32, version: i32)
    -> Result<Version, FindVersionError> {
        edit_processes::table
            .inner_join(edit_process_versions::table)
            .filter(edit_processes::id.eq(process)
                .and(edit_process_versions::id.eq(version)))
            .get_result::<(db::EditProcess, db::EditProcessVersion)>(dbcon)
            .optional()?
            .ok_or(FindVersionError::NotFound)
            .map(|(process, version)| Self::from_db(
                version, Process::from_db(process)))
    }

    pub fn process(&self) -> &Process {
        &self.process
    }

    /// Get public portion of this version's data.
    pub fn get_public(&self) -> PublicData {
        let db::EditProcessVersion { id, version, .. } = self.data;

        PublicData {
            id,
            name: self.process.name.clone(),
            version,
        }
    }
}

impl Deref for Version {
    type Target = db::EditProcessVersion;

    fn deref(&self) -> &db::EditProcessVersion {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindVersionError {
    /// Database error.
    #[api(internal)]
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// No version found matching given criteria.
    #[api(code = "edit-process:not-found", status = "NOT_FOUND")]
    #[fail(display = "No such process")]
    NotFound,
}

impl_from! { for FindVersionError ;
    DbError => |e| FindVersionError::Database(e),
}
