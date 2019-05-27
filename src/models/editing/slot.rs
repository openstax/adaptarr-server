use diesel::{
    prelude::*,
    result::Error as DbError,
};
use failure::Fail;
use serde::Serialize;
use std::ops::Deref;
use uuid::Uuid;

use crate::{
    ApiError,
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
    models::{Document, Draft, user::{User, FindUserError}},
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
    pub role: Option<i32>,
}

impl Slot {
    /// Construct `Slot` from its database counterpart.
    pub(super) fn from_db(data: db::EditProcessSlot) -> Slot {
        Slot { data }
    }

    /// Find a slot by ID.
    pub fn by_id(dbcon: &Connection, id: i32) -> Result<Slot, FindSlotError> {
        edit_process_slots::table
            .filter(edit_process_slots::id.eq(id))
            .get_result::<db::EditProcessSlot>(dbcon)
            .optional()?
            .ok_or(FindSlotError::NotFound)
            .map(Slot::from_db)
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
        let db::EditProcessSlot { id, ref name, role, .. } = self.data;

        PublicData {
            id,
            name: name.clone(),
            role,
        }
    }

    /// Get current occupant of this slot in a draft.
    pub fn get_occupant(&self, dbcon: &Connection, draft: Uuid)
    -> Result<Option<User>, DbError> {
        draft_slots::table
            .filter(draft_slots::draft.eq(draft)
                .and(draft_slots::slot.eq(self.data.id)))
            .get_result::<db::DraftSlot>(dbcon)
            .optional()?
            .map(|slot| match User::by_id(dbcon, slot.user) {
                Ok(user) => Ok(user),
                Err(FindUserError::Internal(err)) => Err(err),
                Err(_) => panic!("Database inconsistency: no user for draft slot"),
            })
            .transpose()
    }

    /// Fill this slot with an auto-selected user for a particular draft.
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

        self.fill_with(dbcon, draft, &user)?;

        Ok(Some(user.id))
    }

    /// Fill this slot with a user for a particular draft.
    pub fn fill_with(&self, dbcon: &Connection, draft: Uuid, user: &db::User)
    -> Result<(), FillSlotError> {
        if let Some(role) = self.data.role {
            if !user.role.map_or(false, |r| r == role) {
                return Err(FillSlotError::BadRole);
            }
        }

        debug!("Assigning {:?} to {:?}", user, self.data);

        diesel::insert_into(draft_slots::table)
            .values(db::DraftSlot {
                draft,
                slot: self.data.id,
                user: user.id,
            })
            .on_conflict((draft_slots::draft, draft_slots::slot))
            .do_update()
            .set(draft_slots::user.eq(user.id))
            .execute(dbcon)?;

        Ok(())
    }
}

impl Deref for Slot {
    type Target = db::EditProcessSlot;

    fn deref(&self) -> &db::EditProcessSlot {
        &self.data
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
    /// User doesn't have required role.
    #[api(code = "edit-process:slot:fill:bad-role", status = "BAD_REQUEST")]
    #[fail(display = "User doesn't have required role")]
    BadRole,
}

impl_from! { for FillSlotError ;
    DbError => |e| FillSlotError::Database(e),
}
