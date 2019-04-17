use chrono::NaiveDateTime;
use diesel::{
    Connection as _,
    prelude::*,
    result::Error as DbError,
};
use std::ops::Deref;

use crate::db::{
    Connection,
    models as db,
    schema::{
        edit_process_links,
        edit_process_slots,
        edit_process_step_slots,
        edit_process_steps,
        edit_process_versions,
        edit_processes,
    },
};
use super::{Process, structure};

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

    /// Get a complete description of this editing process.
    pub fn get_structure(&self, dbcon: &Connection)
    -> Result<structure::Process, DbError> {
        dbcon.transaction(|| {
            let dbslots = edit_process_slots::table
                .filter(edit_process_slots::process.eq(self.data.id))
                .get_results::<db::EditProcessSlot>(dbcon)?;

            let slots = dbslots.iter()
                .map(|slot| structure::Slot {
                    id: slot.id,
                    name: slot.name.clone(),
                    role: slot.role,
                    autofill: slot.autofill,
                })
                .collect();

            let dbsteps = edit_process_steps::table
                .filter(edit_process_steps::process.eq(self.data.id))
                .get_results::<db::EditProcessStep>(dbcon)?;

            let start = dbsteps.iter()
                .position(|step| step.id == self.data.start)
                .expect("database inconsistency: no start step");

            let steps = dbsteps.iter()
                .map(|step| {
                    let slots = edit_process_step_slots::table
                        .filter(edit_process_step_slots::step.eq(step.id))
                        .get_results::<db::EditProcessStepSlot>(dbcon)?
                        .into_iter()
                        .map(|slot| structure::StepSlot {
                            slot: dbslots.iter()
                                .position(|s2| s2.id == slot.slot)
                                .expect(
                                    "database inconsistency: no slot for step"),
                            permission: slot.permission,
                        })
                        .collect();

                    let links = edit_process_links::table
                        .filter(edit_process_links::from.eq(step.id))
                        .get_results::<db::EditProcessLink>(dbcon)?
                        .into_iter()
                        .map(|link| {
                            let to = dbsteps.iter()
                                .position(|l2| l2.id == link.to)
                                .expect(
                                    "database inconsistency: no target for link");

                            let slot = dbslots.iter()
                                .position(|slot| slot.id == link.slot)
                                .expect(
                                    "database inconsistency: no slot for link");

                            structure::Link {
                                name: link.name,
                                to,
                                slot,
                            }
                        })
                        .collect();

                    Ok(structure::Step {
                        id: step.id,
                        name: step.name.clone(),
                        slots,
                        links,
                    })
                })
                .collect::<Result<_, DbError>>()?;

            Ok(structure::Process {
                name: self.process.name.clone(),
                start,
                slots,
                steps,
            })
        })
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
