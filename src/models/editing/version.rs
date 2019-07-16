use chrono::{NaiveDateTime, Utc};
use diesel::{
    Connection as _,
    prelude::*,
    result::{Error as DbError, DatabaseErrorKind},
};
use failure::Fail;
use serde::Serialize;
use std::ops::Deref;

use crate::{
    ApiError,
    audit,
    db::{
        Connection,
        models as db,
        schema::{
            edit_process_slot_roles,
            edit_process_links,
            edit_process_slots,
            edit_process_step_slots,
            edit_process_steps,
            edit_process_versions,
            edit_processes,
        },
    },
};
use super::{Process, Slot, structure};

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

    /// Create a new version of an editing process.
    pub fn create(
        dbcon: &Connection,
        process: Process,
        structure: &structure::Process,
    ) -> Result<Version, CreateVersionError> {
        let _ = structure::validate(structure)?;
        let process = process.into_db();

        dbcon.transaction(|| {
            dbcon.execute("set constraints all deferred")?;

            let version = diesel::insert_into(edit_process_versions::table)
                .values(&db::NewEditProcessVersion {
                    process: process.id,
                    version: Utc::now().naive_utc(),
                    start: 0,
                })
                .get_result::<db::EditProcessVersion>(dbcon)?;

            let slots = structure.slots.iter()
                .map(|slot| {
                    let data = diesel::insert_into(edit_process_slots::table)
                        .values(&db::NewEditProcessSlot {
                            process: version.id,
                            name: &slot.name,
                            autofill: slot.autofill,
                        })
                        .get_result::<db::EditProcessSlot>(dbcon)?;

                    diesel::insert_into(edit_process_slot_roles::table)
                        .values(slot.roles.iter()
                            .map(|&role| db::EditProcessSlotRole {
                                slot: data.id,
                                role,
                            })
                            .collect::<Vec<_>>())
                        .execute(dbcon)?;

                    Ok(data)
                })
                .collect::<Result<Vec<db::EditProcessSlot>, DbError>>()?;

            let steps = structure.steps.iter()
                .map(|step| {
                    diesel::insert_into(edit_process_steps::table)
                        .values(&db::NewEditProcessStep {
                            name: &step.name,
                            process: version.id,
                        })
                        .get_result::<db::EditProcessStep>(dbcon)
                })
                .collect::<Result<Vec<db::EditProcessStep>, _>>()?;

            for (step, dbstep) in structure.steps.iter().zip(steps.iter()) {
                for slot in &step.slots {
                    diesel::insert_into(edit_process_step_slots::table)
                        .values(&db::EditProcessStepSlot {
                            step: dbstep.id,
                            slot: slots[slot.slot].id,
                            permission: slot.permission,
                        })
                        .execute(dbcon)?;
                }

                for link in &step.links {
                    diesel::insert_into(edit_process_links::table)
                        .values(&db::NewEditProcessLink {
                            name: &link.name,
                            from: dbstep.id,
                            to: steps[link.to].id,
                            slot: slots[link.slot].id,
                        })
                        .execute(dbcon)?;
                }
            }

            let version = diesel::update(&version)
                .set(edit_process_versions::start.eq(steps[structure.start].id))
                .get_result::<db::EditProcessVersion>(dbcon)?;

            let process = if structure.name != process.name {
                diesel::update(&process)
                    .set(edit_processes::name.eq(&structure.name))
                    .get_result(dbcon)
                    .map(Process::from_db)?
            } else {
                Process::from_db(process)
            };

            audit::log_db(
                dbcon, "edit-process", process.id, "create-version", version.id);

            Ok(Version::from_db(version, process))
        })
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

    /// Get list of all slots in this process.
    pub fn get_slots(&self, dbcon: &Connection) -> Result<Vec<Slot>, DbError> {
        edit_process_slots::table
            .filter(edit_process_slots::process.eq(self.data.id))
            .get_results(dbcon)
            .map(|v| v.into_iter().map(Slot::from_db).collect())
    }

    /// Find a slot in this process.
    pub fn get_slot(&self, dbcon: &Connection, id: i32)
    -> Result<Slot, FindSlotError> {
        edit_process_slots::table
            .filter(edit_process_slots::id.eq(id)
                .and(edit_process_slots::process.eq(self.data.id)))
            .get_result(dbcon)
            .optional()?
            .ok_or(FindSlotError::NotFound)
            .map(Slot::from_db)
    }

    /// Get a complete description of this editing process.
    pub fn get_structure(&self, dbcon: &Connection)
    -> Result<structure::Process, DbError> {
        dbcon.transaction(|| {
            let dbslots = edit_process_slots::table
                .filter(edit_process_slots::process.eq(self.data.id))
                .get_results::<db::EditProcessSlot>(dbcon)?;

            let slots = dbslots.iter()
                .map(|slot| Ok(structure::Slot {
                    id: slot.id,
                    name: slot.name.clone(),
                    roles: edit_process_slot_roles::table
                        .select(edit_process_slot_roles::role)
                        .filter(edit_process_slot_roles::slot.eq(slot.id))
                        .get_results::<i32>(dbcon)?,
                    autofill: slot.autofill,
                }))
                .collect::<Result<Vec<_>, DbError>>()?;

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

#[derive(ApiError, Debug, Fail)]
pub enum CreateVersionError {
    /// Description of a process is not valid.
    #[api(code = "edit-process:new:invalid-description", status = "BAD_REQUEST")]
    #[fail(display = "{}", _0)]
    InvalidDescription(#[cause] structure::ValidateStructureError),
    /// Database error
    #[api(internal)]
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// Duplicate process
    #[api(code = "edit-process:new:exists", status = "BAD_REQUEST")]
    #[fail(display = "A process with this name already exists")]
    Duplicate,
}

impl_from! { for CreateVersionError ;
    structure::ValidateStructureError => |e| CreateVersionError::InvalidDescription(e),
    DbError => |e| match e {
        DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, detail) =>
            match detail.constraint_name() {
                Some("edit_processes_name_key") => CreateVersionError::Duplicate,
                _ => CreateVersionError::Database(DbError::DatabaseError(
                    DatabaseErrorKind::UniqueViolation, detail)),
            },
        _ => CreateVersionError::Database(e),
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindSlotError {
    /// Database error.
    #[api(internal)]
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// No slot found matching given criteria.
    #[api(code = "edit-process:slot:not-found", status = "NOT_FOUND")]
    #[fail(display = "No such slot")]
    NotFound,
}

impl_from! { for FindSlotError ;
    DbError => |e| FindSlotError::Database(e),
}
