use diesel::{
    Connection as _,
    prelude::*,
    result::Error as DbError,
};
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
};
use super::{
    Slot,
    link::{Link, PublicData as LinkData, FindLinkError},
    version::{Version, FindVersionError},
};

/// A single step in an editing process.
///
/// See [module description][super] for details.
#[derive(Debug)]
pub struct Step {
    data: db::EditProcessStep,
}

/// A subset of this step's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub id: i32,
    pub process: [i32; 2],
    pub name: String,
    pub links: Vec<LinkData>,
}

impl Step {
    /// Construct `Step` from its database counterpart.
    pub(super) fn from_db(data: db::EditProcessStep) -> Step {
        Step { data }
    }

    /// Find step by ID.
    pub fn by_id(dbcon: &Connection, id: i32) -> Result<Step, DbError> {
        edit_process_steps::table
            .filter(edit_process_steps::id.eq(id))
            .get_result(dbcon)
            .map(Step::from_db)
    }

    /// Unpack database data.
    pub fn into_db(self) -> db::EditProcessStep {
        self.data
    }

    /// Check whether this is a final step.
    pub fn is_final(&self, dbcon: &Connection) -> Result<bool, DbError> {
        edit_process_links::table
            .select(diesel::dsl::count(edit_process_links::to))
            .filter(edit_process_links::from.eq(self.data.id))
            .get_result::<i64>(dbcon)
            .map(|c| c == 0)
    }

    /// Get the process this step is a part of.
    pub fn get_process(&self, dbcon: &Connection) -> Result<Version, DbError> {
        let process = edit_process_versions::table
            .filter(edit_process_versions::id.eq(self.data.process))
            .select(edit_process_versions::process)
            .get_result(dbcon)?;

        match Version::by_id(dbcon, process, self.data.process) {
            Ok(v) => Ok(v),
            Err(FindVersionError::Database(err)) => Err(err),
            Err(FindVersionError::NotFound) =>
                panic!("Inconsistent database: no version for step"),
        }
    }

    /// Get list of slots and permissions they have during this step.
    pub fn get_slots(&self, dbcon: &Connection)
    -> Result<Vec<(Slot, Vec<SlotPermission>)>, DbError> {
        let slots = edit_process_step_slots::table
            .inner_join(edit_process_slots::table)
            .filter(edit_process_step_slots::step.eq(self.data.id))
            .order_by(edit_process_step_slots::slot.desc())
            .get_results::<(db::EditProcessStepSlot, db::EditProcessSlot)>(dbcon)?
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
    pub fn get_slot_seating(&self, dbcon: &Connection, draft: Uuid)
    -> Result<Vec<(Slot, Vec<SlotPermission>, Option<i32>)>, DbError> {
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
            )>(dbcon)?
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
                (
                    Slot::from_db(key.unwrap()),
                    permissions,
                    seating.map(|s| s.user),
                )
            })
            .collect();

        Ok(slots)
    }

    /// Get list of list originating at this step. The list can optionally
    /// be limited to just links usable by a specific slots.
    pub fn get_links(&self, dbcon: &Connection, slots: Option<(Uuid, i32)>)
    -> Result<Vec<Link>, DbError> {
        if let Some((draft, user)) = slots {
            edit_process_links::table
                .inner_join(draft_slots::table
                    .on(edit_process_links::slot.eq(draft_slots::slot)))
                .filter(edit_process_links::from.eq(self.data.id)
                    .and(draft_slots::draft.eq(draft))
                    .and(draft_slots::user.eq(user)))
                .get_results::<(db::EditProcessLink, db::DraftSlot)>(dbcon)
                .map(|v| {
                    v.into_iter()
                        .map(|(link, _)| Link::from_db(link))
                        .collect()
                })
        } else {
            edit_process_links::table
                .filter(edit_process_links::from.eq(self.data.id))
                .get_results(dbcon)
                .map(|v| v.into_iter().map(Link::from_db).collect())
        }
    }

    pub fn get_link(&self, dbcon: &Connection, slot: i32, target: i32)
    -> Result<Link, FindLinkError> {
        edit_process_links::table
            .filter(edit_process_links::from.eq(self.data.id)
                .and(edit_process_links::to.eq(target))
                .and(edit_process_links::slot.eq(slot)))
            .get_result(dbcon)
            .optional()?
            .map(Link::from_db)
            .ok_or(FindLinkError::NotFound)
    }

    /// Get the public portion of this step's data. The list can optionally
    /// be limited to just the data visible by a specific slots.
    pub fn get_public(&self, dbcon: &Connection, slots: Option<(Uuid, i32)>)
    -> Result<PublicData, DbError> {
        let db::EditProcessStep { id, process: version, ref name, .. } = self.data;

        let links = self.get_links(dbcon, slots)?
            .iter()
            .map(Link::get_public)
            .collect();

        let process = edit_process_versions::table
            .filter(edit_process_versions::id.eq(version))
            .select(edit_process_versions::process)
            .get_result(dbcon)?;

        Ok(PublicData {
            id,
            process: [process, version],
            name: name.clone(),
            links,
        })
    }

    /// Set step's name.
    pub fn set_name(&mut self, dbcon: &Connection, name: &str) -> Result<(), DbError> {
        dbcon.transaction(|| {
            audit::log_db(dbcon, "steps", self.data.id, "set-name", name);

            self.data = diesel::update(&self.data)
                .set(edit_process_steps::name.eq(name))
                .get_result(dbcon)?;

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
