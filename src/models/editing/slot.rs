use diesel::{
    Connection as _,
    prelude::*,
    result::Error as DbError,
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
            documents,
            draft_slots,
            drafts,
            edit_process_slot_roles,
            edit_process_slots,
            edit_process_step_slots,
            users,
        },
        functions::count_distinct,
    },
    events::{self, EventManager},
    models::{Document, Draft, Role, user::{User, FindUserError}},
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
    pub roles: Vec<i32>,
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
                .on(edit_process_step_slots::slot.eq(edit_process_slots::id)))
            .left_join(edit_process_slot_roles::table
                .on(edit_process_slots::id.eq(edit_process_slot_roles::slot)))
            .select((
                drafts::all_columns,
                documents::all_columns,
                edit_process_slots::all_columns,
            ));

        let slots = if let Some(role) = role {
            query
                .filter(draft_slots::user.is_null()
                    .and(edit_process_slot_roles::role.is_null()
                        .or(edit_process_slot_roles::role.eq(role))))
                .get_results::<(
                    db::Draft,
                    db::Document,
                    db::EditProcessSlot,
                )>(dbcon)?
        } else {
            query
                .filter(draft_slots::user.is_null()
                    .and(edit_process_slot_roles::role.is_null()))
                .get_results::<(
                    db::Draft,
                    db::Document,
                    db::EditProcessSlot,
                )>(dbcon)?
        };

        Ok(slots
            .into_iter()
            .map(|(draft, document, slot)| (
                Draft::from_db(draft, Document::from_db(document)),
                Slot::from_db(slot),
            ))
            .collect())
    }

    /// Does a draft have any unoccupied slots to which a user with given role
    /// can assign themselves?
    pub fn free_in_draft_for(dbcon: &Connection, draft: &Draft, role: Option<i32>)
    -> Result<bool, DbError> {
        // TODO: fold this and all_free into a single helper function?
        let query = drafts::table
            .inner_join(edit_process_step_slots::table
                .on(drafts::step.eq(edit_process_step_slots::step)))
            .left_join(draft_slots::table
                .on(drafts::module.eq(draft_slots::draft)
                    .and(edit_process_step_slots::slot.eq(draft_slots::slot))))
            .inner_join(edit_process_slots::table
                .on(edit_process_step_slots::slot.eq(edit_process_slots::id)))
            .left_join(edit_process_slot_roles::table
                .on(edit_process_slots::id.eq(edit_process_slot_roles::slot)));

        let count = if let Some(role) = role {
            query
                .filter(draft_slots::user.is_null()
                    .and(edit_process_slot_roles::role.is_null()
                        .or(edit_process_slot_roles::role.eq(role)))
                    .and(drafts::module.eq(draft.module_id())))
                .count()
                .get_result::<i64>(dbcon)?
        } else {
            query
                .filter(draft_slots::user.is_null()
                    .and(edit_process_slot_roles::role.is_null())
                    .and(drafts::module.eq(draft.module_id())))
                .count()
                .get_result::<i64>(dbcon)?
        };

        Ok(count > 0)
    }

    /// Unpack database data.
    pub fn into_db(self) -> db::EditProcessSlot {
        self.data
    }

    /// Get the public portion of this slot's data.
    pub fn get_public(&self, dbcon: &Connection) -> Result<PublicData, DbError> {
        let db::EditProcessSlot { id, ref name, .. } = self.data;

        Ok(PublicData {
            id,
            name: name.clone(),
            roles: self.get_role_limit(dbcon)?,
        })
    }

    /// Get current occupant of this slot in a draft.
    pub fn get_occupant(&self, dbcon: &Connection, draft: &Draft)
    -> Result<Option<User>, DbError> {
        draft_slots::table
            .filter(draft_slots::draft.eq(draft.module_id())
                .and(draft_slots::slot.eq(self.data.id)))
            .get_result::<db::DraftSlot>(dbcon)
            .optional()?
            .map(|slot| match User::by_id(dbcon, slot.user) {
                Ok(user) => Ok(user),
                Err(FindUserError::Internal(err)) => Err(err),
                Err(FindUserError::NotFound) =>
                    panic!("Database inconsistency: no user for draft slot"),
            })
            .transpose()
    }

    /// Get list of roles to which this slot is limited.
    ///
    /// If the list is empty the there is no limit on this slot.
    pub fn get_role_limit(&self, dbcon: &Connection) -> Result<Vec<i32>, DbError> {
        edit_process_slot_roles::table
            .select(edit_process_slot_roles::role)
            .filter(edit_process_slot_roles::slot.eq(self.data.id))
            .get_results::<i32>(dbcon)
    }

    /// Is this slot limited to only users with a specific role?
    pub fn is_role_limited(&self, dbcon: &Connection) -> Result<bool, DbError> {
        edit_process_slot_roles::table
            .filter(edit_process_slot_roles::slot.eq(self.data.id))
            .limit(1)
            .count()
            .get_result::<i64>(dbcon)
            .map(|r| r != 0)
    }

    /// Fill this slot with an auto-selected user for a particular draft.
    pub fn fill(&self, dbcon: &Connection, draft: &Draft)
    -> Result<Option<i32>, FillSlotError> {
        if !self.data.autofill {
            return Ok(None);
        }

        let user = users::table
            .inner_join(edit_process_slot_roles::table
                .on(users::role.eq(edit_process_slot_roles::role.nullable())))
            .inner_join(draft_slots::table)
            .select(users::all_columns)
            .group_by((users::all_columns, draft_slots::draft))
            .order_by(count_distinct(draft_slots::draft).asc())
            .first::<db::User>(dbcon)
            .optional()?
            .ok_or(FillSlotError::NoUser)?;

        self.fill_with(dbcon, draft, &user)?;

        Ok(Some(user.id))
    }

    /// Fill this slot with a user for a particular draft.
    pub fn fill_with(&self, dbcon: &Connection, draft: &Draft, user: &db::User)
    -> Result<(), FillSlotError> {
        let roles = self.get_role_limit(dbcon)?;

        if !roles.is_empty() {
            if !user.role.map_or(false, |r| roles.iter().any(|&role| r == role)) {
                return Err(FillSlotError::BadRole);
            }
        }

        debug!("Assigning {:?} to {:?}", user, self.data);

        let old = draft_slots::table
            .filter(draft_slots::draft.eq(draft.module_id())
                .and(draft_slots::slot.eq(self.data.id)))
            .select(draft_slots::user.nullable())
            .get_result::<Option<i32>>(dbcon)
            .optional()?
            .unwrap_or(None);

        diesel::insert_into(draft_slots::table)
            .values(db::DraftSlot {
                draft: draft.module_id(),
                slot: self.data.id,
                user: user.id,
            })
            .on_conflict((draft_slots::draft, draft_slots::slot))
            .do_update()
            .set(draft_slots::user.eq(user.id))
            .execute(dbcon)?;

        EventManager::notify(user, events::SlotFilled {
            slot: self.data.id,
            module: draft.module_id(),
            document: draft.id,
        });

        if let Some(old) = old {
            EventManager::notify(old, events::SlotVacated {
                slot: self.data.id,
                module: draft.module_id(),
                document: draft.id,
            });
        }

        audit::log_db(dbcon, "drafts", draft.module_id(), "fill-slot", LogFill {
            slot: self.data.id,
            user: user.id,
        });

        Ok(())
    }

    /// Set slot's name.
    pub fn set_name(&mut self, dbcon: &Connection, name: &str) -> Result<(), DbError> {
        dbcon.transaction(|| {
            audit::log_db(dbcon, "slots", self.id, "set-name", name);

            self.data = diesel::update(&self.data)
                .set(edit_process_slots::name.eq(name))
                .get_result(dbcon)?;

            Ok(())
        })
    }

    /// Set slot's role limit.
    pub fn set_role_limit(&mut self, dbcon: &Connection, roles: &[Role])
    -> Result<(), DbError> {
    let roles = roles.iter().map(|r| r.id).collect::<Vec<_>>();

        dbcon.transaction(|| {
            audit::log_db(dbcon, "slots", self.id, "set-roles", &roles);

            diesel::delete(edit_process_slot_roles::table
                .filter(edit_process_slot_roles::slot.eq(self.data.id))
            ).execute(dbcon)?;

            diesel::insert_into(edit_process_slot_roles::table)
                .values(roles.iter().map(|&role| db::EditProcessSlotRole {
                    slot: self.data.id,
                    role,
                }).collect::<Vec<_>>())
                .execute(dbcon)?;

            Ok(())
        })
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

#[derive(Serialize)]
struct LogFill {
    slot: i32,
    user: i32,
}
