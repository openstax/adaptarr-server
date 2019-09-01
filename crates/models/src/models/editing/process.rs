use diesel::{Connection as _, prelude::*, result::Error as DbError};
use serde::Serialize;

use crate::{
    audit,
    db::{
        Connection,
        models as db,
        schema::{edit_process_versions, edit_processes},
    },
    models::{FindModelResult, Model},
};
use super::{CreateVersionError, Version, structure};

/// An editing process.
///
/// See [module description][super] for details.
#[derive(Clone, Debug)]
pub struct Process {
    data: db::EditProcess,
}

/// A subset of role's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    pub id: i32,
    pub name: String,
}

impl Model for Process {
    const ERROR_CATEGORY: &'static str = "edit-process";

    type Id = i32;
    type Database = db::EditProcess;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, id: Self::Id) -> FindModelResult<Self> {
        edit_processes::table
            .filter(edit_processes::id.eq(id))
            .get_result::<db::EditProcess>(db)
            .map(Process::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Process { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Self::Public {
        let db::EditProcess { id, ref name, .. } = self.data;

        Public {
            id,
            name: name.clone(),
        }
    }
}

impl Process {
    /// Get all modules.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all(db: &Connection) -> Result<Vec<Process>, DbError> {
        edit_processes::table
            .get_results::<db::EditProcess>(db)
            .map(|v| v.into_iter().map(Process::from_db).collect())
    }

    /// Create a new editing process.
    pub fn create(db: &Connection, structure: &structure::Process)
    -> Result<Version, CreateVersionError> {
        db.transaction(|| {
            let process = diesel::insert_into(edit_processes::table)
                .values(&db::NewEditProcess {
                    name: &structure.name,
                })
                .get_result::<db::EditProcess>(db)?;

            audit::log_db(db, "edit-processes", process.id, "create", ());

            Version::create(db, Self::from_db(process), structure)
        })
    }

    /// Delete this editing process.
    ///
    /// Note that only processes which have never been used can be deleted.
    pub fn delete(self, db: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(db)?;
        audit::log_db(db, "edit-processes", self.id, "delete", ());
        Ok(())
    }

    /// Get list of all versions of this process.
    pub fn get_versions(&self, db: &Connection)
    -> Result<Vec<Version>, DbError> {
        Ok(edit_process_versions::table
            .filter(edit_process_versions::process.eq(self.data.id))
            .order_by(edit_process_versions::version.desc())
            .get_results::<db::EditProcessVersion>(db)?
            .into_iter()
            .map(|version| Version::from_db((self.data.clone(), version)))
            .collect())
    }

    /// Get current (latest) version of this process.
    pub fn get_current(&self, db: &Connection) -> Result<Version, DbError> {
        edit_process_versions::table
            .filter(edit_process_versions::process.eq(self.data.id))
            .order_by(edit_process_versions::version.desc())
            .limit(1)
            .get_result::<db::EditProcessVersion>(db)
            .map(|version| Version::from_db((self.data.clone(), version)))
    }

    /// Set process's name.
    pub fn set_name(&mut self, db: &Connection, name: &str)
    -> Result<(), DbError> {
        self.data = diesel::update(&self.data)
            .set(edit_processes::name.eq(name))
            .get_result(db)?;

        audit::log_db(db, "edit-processes", self.id, "set-name", name);

        Ok(())
    }
}

impl std::ops::Deref for Process {
    type Target = db::EditProcess;

    fn deref(&self) -> &db::EditProcess {
        &self.data
    }
}
