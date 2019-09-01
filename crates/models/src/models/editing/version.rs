use adaptarr_error::ApiError;
use adaptarr_macros::From;
use chrono::{NaiveDateTime, Utc};
use diesel::{
    Connection as _,
    prelude::*,
    result::{Error as DbError, DatabaseErrorKind},
};
use failure::Fail;
use serde::Serialize;

use crate::{
    audit,
    db::{
        Connection,
        models as db,
        schema::{
            edit_process_links,
            edit_process_slot_roles,
            edit_process_slots,
            edit_process_step_slots,
            edit_process_steps,
            edit_process_versions,
            edit_processes,
        },
    },
    models::{FindModelResult, Model},
};
use super::{Process, Step, Slot, structure};

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
pub struct Public {
    pub id: i32,
    pub name: String,
    pub version: NaiveDateTime,
}

impl Model for Version {
    const ERROR_CATEGORY: &'static str = "edit-process:version";

    type Id = (i32, i32);
    type Database = (db::EditProcess, db::EditProcessVersion);
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, (process, version): Self::Id)
    -> FindModelResult<Self> {
        edit_processes::table
            .inner_join(edit_process_versions::table)
            .filter(edit_processes::id.eq(process)
                .and(edit_process_versions::id.eq(version)))
            .get_result::<(db::EditProcess, db::EditProcessVersion)>(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db((process, data): Self::Database) -> Self {
        Version {
            data,
            process: Process::from_db(process),
        }
    }

    fn into_db(self) -> Self::Database {
        (self.process.into_db(), self.data)
    }

    fn id(&self) -> Self::Id {
        (self.process.id(), self.data.id)
    }

    fn get_public(&self) -> Public {
        let db::EditProcessVersion { id, version, .. } = self.data;

        Public {
            id,
            name: self.process.name.clone(),
            version,
        }
    }
}

impl Version {
    /// Create a new version of an editing process.
    pub fn create(
        db: &Connection,
        process: Process,
        structure: &structure::Process,
    ) -> Result<Version, CreateVersionError> {
        let _ = structure::validate(structure)?;
        let process = process.into_db();

        db.transaction(|| {
            db.execute("set constraints all deferred")?;

            let version = diesel::insert_into(edit_process_versions::table)
                .values(&db::NewEditProcessVersion {
                    process: process.id,
                    version: Utc::now().naive_utc(),
                    start: 0,
                })
                .get_result::<db::EditProcessVersion>(db)?;

            let slots = structure.slots.iter()
                .map(|slot| {
                    let data = diesel::insert_into(edit_process_slots::table)
                        .values(&db::NewEditProcessSlot {
                            process: version.id,
                            name: &slot.name,
                            autofill: slot.autofill,
                        })
                        .get_result::<db::EditProcessSlot>(db)?;

                    diesel::insert_into(edit_process_slot_roles::table)
                        .values(slot.roles.iter()
                            .map(|&role| db::EditProcessSlotRole {
                                slot: data.id,
                                role,
                            })
                            .collect::<Vec<_>>())
                        .execute(db)?;

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
                        .get_result::<db::EditProcessStep>(db)
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
                        .execute(db)?;
                }

                for link in &step.links {
                    diesel::insert_into(edit_process_links::table)
                        .values(&db::NewEditProcessLink {
                            name: &link.name,
                            from: dbstep.id,
                            to: steps[link.to].id,
                            slot: slots[link.slot].id,
                        })
                        .execute(db)?;
                }
            }

            let version = diesel::update(&version)
                .set(edit_process_versions::start.eq(steps[structure.start].id))
                .get_result::<db::EditProcessVersion>(db)?;

            let process = if structure.name != process.name {
                diesel::update(&process)
                    .set(edit_processes::name.eq(&structure.name))
                    .get_result(db)?
            } else {
                process
            };

            audit::log_db(
                db, "edit-process", process.id, "create-version", version.id);

            Ok(Version::from_db((process, version)))
        })
    }

    pub fn process(&self) -> &Process {
        &self.process
    }

    /// Get list of all slots in this process.
    pub fn get_slots(&self, db: &Connection) -> Result<Vec<Slot>, DbError> {
        edit_process_slots::table
            .filter(edit_process_slots::process.eq(self.data.id))
            .get_results(db)
            .map(|v| v.into_iter().map(Slot::from_db).collect())
    }

    /// Find a slot in this process.
    pub fn get_slot(&self, db: &Connection, id: i32)
    -> FindModelResult<Slot> {
        edit_process_slots::table
            .filter(edit_process_slots::id.eq(id)
                .and(edit_process_slots::process.eq(self.data.id)))
            .get_result(db)
            .map(Slot::from_db)
            .map_err(From::from)
    }

    /// Get list of all steps in this process.
    pub fn get_steps(&self, db: &Connection) -> Result<Vec<Step>, DbError> {
        edit_process_steps::table
            .filter(edit_process_steps::process.eq(self.data.id))
            .get_results(db)
            .map(|v| v.into_iter().map(Step::from_db).collect())
    }

    /// Find a step in this process.
    pub fn get_step(&self, db: &Connection, id: i32)
    -> FindModelResult<Step> {
        edit_process_steps::table
            .filter(edit_process_steps::id.eq(id)
                .and(edit_process_steps::process.eq(self.data.id)))
            .get_result(db)
            .map(Step::from_db)
            .map_err(From::from)
    }

    /// Get a complete description of this editing process.
    pub fn get_structure(&self, db: &Connection)
    -> Result<structure::Process, DbError> {
        db.transaction(|| {
            let dbslots = edit_process_slots::table
                .filter(edit_process_slots::process.eq(self.data.id))
                .order_by(edit_process_slots::id.asc())
                .get_results::<db::EditProcessSlot>(db)?;

            let slots = dbslots.iter()
                .map(|slot| Ok(structure::Slot {
                    id: slot.id,
                    name: slot.name.clone(),
                    roles: edit_process_slot_roles::table
                        .select(edit_process_slot_roles::role)
                        .filter(edit_process_slot_roles::slot.eq(slot.id))
                        .get_results::<i32>(db)?,
                    autofill: slot.autofill,
                }))
                .collect::<Result<Vec<_>, DbError>>()?;

            let dbsteps = edit_process_steps::table
                .filter(edit_process_steps::process.eq(self.data.id))
                .order_by(edit_process_steps::id.asc())
                .get_results::<db::EditProcessStep>(db)?;

            let start = dbsteps.iter()
                .position(|step| step.id == self.data.start)
                .expect("database inconsistency: no start step");

            let steps = dbsteps.iter()
                .map(|step| {
                    let slots = edit_process_step_slots::table
                        .filter(edit_process_step_slots::step.eq(step.id))
                        .order_by(edit_process_step_slots::slot.asc())
                        .get_results::<db::EditProcessStepSlot>(db)?
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
                        .order_by((
                            edit_process_links::slot.asc(),
                            edit_process_links::to.asc(),
                        ))
                        .get_results::<db::EditProcessLink>(db)?
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

impl std::ops::Deref for Version {
    type Target = db::EditProcessVersion;

    fn deref(&self) -> &db::EditProcessVersion {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum CreateVersionError {
    /// Description of a process is not valid.
    #[api(code = "edit-process:new:invalid-description", status = "BAD_REQUEST")]
    #[fail(display = "{}", _0)]
    InvalidDescription(#[cause] #[from] structure::ValidateStructureError),
    /// Database error
    #[api(internal)]
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// Duplicate process
    #[api(code = "edit-process:new:exists", status = "BAD_REQUEST")]
    #[fail(display = "A process with this name already exists")]
    Duplicate,
}

impl From<DbError> for CreateVersionError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, detail) =>
                match detail.constraint_name() {
                    Some("edit_processes_name_key") => CreateVersionError::Duplicate,
                    _ => CreateVersionError::Database(DbError::DatabaseError(
                        DatabaseErrorKind::UniqueViolation, detail)),
                },
            _ => CreateVersionError::Database(e),
        }
    }
}
