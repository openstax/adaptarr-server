use adaptarr_error::ApiError;
use adaptarr_util::and_tuple;
use diesel::{
    Connection as _,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use failure::Fail;
use itertools::Itertools;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    audit,
    db::{
        Connection,
        models as db,
        schema::{
            draft_slots,
            edit_process_links,
            edit_process_slots,
            edit_process_step_slots,
            edit_process_steps,
            edit_process_versions,
        },
        types::SlotPermission,
    },
    models::{AssertExists, FindModelResult, Model},
};
use super::{Link, Slot, Version};

/// A single step in an editing process.
///
/// See [module description][super] for details.
#[derive(Debug)]
pub struct Step {
    data: db::EditProcessStep,
}

#[derive(Debug)]
pub struct Seating {
    pub slot: Slot,
    pub permissions: Vec<SlotPermission>,
    pub user: Option<i32>,
}

/// A subset of this step's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    pub id: i32,
    pub process: [i32; 2],
    pub name: String,
    pub slots: Vec<StepSlot>,
    pub links: Vec<<Link as Model>::Public>,
}

#[derive(Debug, Serialize)]
pub struct StepSlot {
    pub slot: i32,
    pub permissions: Vec<SlotPermission>,
    pub user: Option<i32>,
}

impl Model for Step {
    const ERROR_CATEGORY: &'static str = "edit-process:step";

    type Id = i32;
    type Database = db::EditProcessStep;
    type Public = Public;
    type PublicParams = (Option<Uuid>, Option<i32>);

    fn by_id(db: &Connection, id: Self::Id) -> FindModelResult<Self> {
        edit_process_steps::table
            .filter(edit_process_steps::id.eq(id))
            .get_result(db)
            .map_err(From::from)
            .map(Step::from_db)
    }

    fn from_db(data: Self::Database) -> Self {
        Step { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Public {
        let db = crate::db::pool().get().expect("uninitialized database");
        self.get_public_full(&*db, &(None, None)).expect("database error")
    }

    fn get_public_full(&self, db: &Connection, &(draft, slots): &Self::PublicParams)
    -> Result<Public, DbError> {
        let db::EditProcessStep { id, process: version, ref name, .. } = self.data;

        let seating = match draft {
            Some(draft) => self.get_slot_seating(db, draft)?
                .into_iter()
                .map(|s| StepSlot {
                    slot: s.slot.id,
                    permissions: s.permissions,
                    user: s.user,
                })
                .collect(),
            None => self.get_slots(db)?
                .into_iter()
                .map(|(slot, permissions)| StepSlot {
                    slot: slot.id,
                    permissions,
                    user: None,
                })
                .collect(),
        };

        let links = self.get_links(db, and_tuple(draft, slots))?
            .iter()
            .map(Link::get_public)
            .collect();

        let process = edit_process_versions::table
            .filter(edit_process_versions::id.eq(version))
            .select(edit_process_versions::process)
            .get_result(db)?;

        Ok(Public {
            id,
            process: [process, version],
            name: name.clone(),
            slots: seating,
            links,
        })
    }
}

impl Step {
    /// Check whether this is a final step.
    pub fn is_final(&self, db: &Connection) -> Result<bool, DbError> {
        edit_process_links::table
            .select(diesel::dsl::count(edit_process_links::to))
            .filter(edit_process_links::from.eq(self.data.id))
            .get_result::<i64>(db)
            .map(|c| c == 0)
    }

    /// Get the process this step is a part of.
    pub fn get_process(&self, db: &Connection) -> Result<Version, DbError> {
        let process = edit_process_versions::table
            .filter(edit_process_versions::id.eq(self.data.process))
            .select(edit_process_versions::process)
            .get_result(db)?;

        Version::by_id(db, (process, self.data.process)).assert_exists()
    }

    /// Get list of slots and permissions they have during this step.
    pub fn get_slots(&self, db: &Connection)
    -> Result<Vec<(Slot, Vec<SlotPermission>)>, DbError> {
        let slots = edit_process_step_slots::table
            .inner_join(edit_process_slots::table)
            .filter(edit_process_step_slots::step.eq(self.data.id))
            .order_by(edit_process_step_slots::slot.desc())
            .get_results::<(db::EditProcessStepSlot, db::EditProcessSlot)>(db)?
            .into_iter()
            .group_by(|(_, slot)| slot.id)
            .into_iter()
            .map(|(_, items)| {
                let mut key = None;

                let permissions = items
                    .map(|(ss, slot)| {
                        key = Some(slot);
                        ss.permission
                    })
                    .collect();

                // Every group has at least one element, so we know key cannot
                // be None.
                (Slot::from_db(key.unwrap()), permissions)
            })
            .collect();

        Ok(slots)
    }

    /// Get list of slots, permissions they have during this step, and IDs
    /// of users occupying these slots.
    #[allow(clippy::type_complexity)]
    pub fn get_slot_seating(&self, db: &Connection, draft: Uuid)
    -> Result<Vec<Seating>, DbError> {
        let slots = edit_process_step_slots::table
            .inner_join(edit_process_slots::table
                .left_join(draft_slots::table))
            .filter(edit_process_step_slots::step.eq(self.data.id)
                .and(draft_slots::draft.eq(draft)))
            .order_by((
                edit_process_step_slots::slot.desc(),
                draft_slots::user.desc(),
            ))
            .get_results::<(
                db::EditProcessStepSlot,
                (db::EditProcessSlot, Option<db::DraftSlot>),
            )>(db)?
            .into_iter()
            .group_by(|(_, (slot, seating))| (slot.id, seating.map(|s| s.user)))
            .into_iter()
            .map(|(_, items)| {
                let mut key = None;
                let mut seating = None;

                let permissions = items
                    .map(|(ss, (slot, seat))| {
                        key = Some(slot);
                        seating = seat;
                        ss.permission
                    })
                    .collect();

                // Every group has at least one element, so we know key and
                // seating cannot be None.
                Seating {
                    slot: Slot::from_db(key.unwrap()),
                    permissions,
                    user: seating.map(|s| s.user),
                }
            })
            .collect();

        Ok(slots)
    }

    /// Get list of list originating at this step. The list can optionally
    /// be limited to just links usable by a specific slots.
    pub fn get_links(&self, db: &Connection, slots: Option<(Uuid, i32)>)
    -> Result<Vec<Link>, DbError> {
        if let Some((draft, user)) = slots {
            edit_process_links::table
                .inner_join(draft_slots::table
                    .on(edit_process_links::slot.eq(draft_slots::slot)))
                .filter(edit_process_links::from.eq(self.data.id)
                    .and(draft_slots::draft.eq(draft))
                    .and(draft_slots::user.eq(user)))
                .get_results::<(db::EditProcessLink, db::DraftSlot)>(db)
                .map(|v| {
                    v.into_iter()
                        .map(|(link, _)| Link::from_db(link))
                        .collect()
                })
        } else {
            edit_process_links::table
                .filter(edit_process_links::from.eq(self.data.id))
                .get_results(db)
                .map(|v| v.into_iter().map(Link::from_db).collect())
        }
    }

    pub fn get_link(&self, db: &Connection, slot: i32, target: i32)
    -> FindModelResult<Link> {
        edit_process_links::table
            .filter(edit_process_links::from.eq(self.data.id)
                .and(edit_process_links::to.eq(target))
                .and(edit_process_links::slot.eq(slot)))
            .get_result(db)
            .map(Link::from_db)
            .map_err(From::from)
    }

    /// Set step's name.
    pub fn set_name(&mut self, db: &Connection, name: &str)
    -> Result<(), RenameStepError> {
        db.transaction(|| {
            audit::log_db(db, "steps", self.data.id, "set-name", name);

            self.data = diesel::update(&self.data)
                .set(edit_process_steps::name.eq(name))
                .get_result(db)?;

            Ok(())
        })
    }
}

impl std::ops::Deref for Step {
    type Target = db::EditProcessStep;

    fn deref(&self) -> &db::EditProcessStep {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum RenameStepError {
    #[api(internal)]
    #[fail(display = "{}", _0)]
    Database(#[cause] DbError),
    #[api(code = "edit-process:step:name:duplicate", status = "BAD_REQUEST")]
    #[fail(display = "rename would result in a duplicate name")]
    DuplicateName,
}

impl From<DbError> for RenameStepError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) =>
                Self::DuplicateName,
            _ => Self::Database(err),
        }
    }
}
