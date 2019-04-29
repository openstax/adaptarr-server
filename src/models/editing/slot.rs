use diesel::{
    prelude::*,
    result::Error as DbError,
};
use std::ops::Deref;
use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::{draft_slots, users},
    functions::count_distinct,
};

/// Abstract representation of roles a user can take during an editing process.
#[derive(Debug)]
pub struct Slot {
    data: db::EditProcessSlot,
}

impl Slot {
    /// Construct `Slot` from its database counterpart.
    pub(super) fn from_db(data: db::EditProcessSlot) -> Slot {
        Slot { data }
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
