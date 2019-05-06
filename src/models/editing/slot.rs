use diesel::{
    prelude::*,
    result::Error as DbError,
};
use std::ops::Deref;
use uuid::Uuid;

use crate::{
    db::{
        Connection,
        models as db,
        schema::{
            edit_process_slots,
            edit_process_step_slots,
            drafts,
            documents,
            draft_slots,
            users,
        },
        functions::count_distinct,
    },
    models::{Document, Draft},
};

/// Abstract representation of roles a user can take during an editing process.
#[derive(Debug)]
pub struct Slot {
    data: db::EditProcessSlot,
}

/// A subset of slot's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub id: i32,
    pub name: String,
}

impl Slot {
    /// Construct `Slot` from its database counterpart.
    pub(super) fn from_db(data: db::EditProcessSlot) -> Slot {
        Slot { data }
    }

    /// Get list of all currently unoccupied slots to which a user with given
    /// role can assign themselves.
    pub fn all_free(dbcon: &Connection, role: Option<i32>)
    -> Result<Vec<(Draft, Slot)>, DbError> {
        let query = drafts::table
            .inner_join(edit_process_step_slots::table
                .on(drafts::step.eq(edit_process_step_slots::step)))
            .left_join(draft_slots::table
                .on(drafts::module.eq(draft_slots::draft)
                    .and(edit_process_step_slots::slot.eq(draft_slots::slot))))
            .inner_join(documents::table)
            .inner_join(edit_process_slots::table
                .on(edit_process_step_slots::slot.eq(edit_process_slots::id)));

        let slots = if let Some(role) = role {
            query
                .filter(draft_slots::user.is_null()
                    .and(edit_process_slots::role.is_null()
                        .or(edit_process_slots::role.eq(role))))
                .get_results::<(
                    db::Draft,
                    db::EditProcessStepSlot,
                    Option<db::DraftSlot>, // Note: this will always be None.
                    db::Document,
                    db::EditProcessSlot,
                )>(dbcon)?
        } else {
            query
                .filter(draft_slots::user.is_null()
                    .and(edit_process_slots::role.is_null()))
                .get_results::<(
                    db::Draft,
                    db::EditProcessStepSlot,
                    Option<db::DraftSlot>, // Note: this will always be None.
                    db::Document,
                    db::EditProcessSlot,
                )>(dbcon)?
        };

        Ok(slots
            .into_iter()
            .map(|(draft, _, _, document, slot)| (
                Draft::from_db(draft, Document::from_db(document)),
                Slot::from_db(slot),
            ))
            .collect())
    }

    /// Get the public portion of this slot's data.
    pub fn get_public(&self) -> PublicData {
        let db::EditProcessSlot { id, ref name, .. } = self.data;

        PublicData {
            id,
            name: name.clone(),
        }
    }

    /// Fill this slot with a user for a particular draft.
    pub fn fill(&self, dbcon: &Connection, draft: Uuid)
    -> Result<Option<i32>, FillSlotError> {
        if !self.data.autofill {
            return Ok(None);
        }

        let user = users::table
            .inner_join(draft_slots::table)
            .select(users::all_columns)
            .filter(users::role.eq(self.data.role))
            .group_by((users::all_columns, draft_slots::draft))
            .order_by(count_distinct(draft_slots::draft).asc())
            .first::<db::User>(dbcon)
            .optional()?
            .ok_or(FillSlotError::NoUser)?;

        diesel::insert_into(draft_slots::table)
            .values(db::DraftSlot {
                draft,
                slot: self.data.id,
                user: user.id,
            })
            .execute(dbcon)?;

        Ok(Some(user.id))
    }
}

impl Deref for Slot {
    type Target = db::EditProcessSlot;

    fn deref(&self) -> &db::EditProcessSlot {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FillSlotError {
    /// Database error
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// There is no user available to fill this slot
    #[fail(display = "There is no user available to fill this slot")]
    #[api(code = "edit-process:slot:fill")]
    NoUser,
}

impl_from! { for FillSlotError ;
    DbError => |e| FillSlotError::Database(e),
}
