use adaptarr_error::ApiError;
use adaptarr_macros::From;
use diesel::{
    Connection as _,
    prelude::*,
    expression::dsl::exists,
    result::{DatabaseErrorKind, Error as DbError},
};
use failure::Fail;
use log::debug;
use serde::Serialize;
use std::ops::Deref;

use crate::{
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
            edit_process_steps,
            edit_process_versions,
            edit_processes,
            team_members,
            users,
        },
        functions::count_distinct,
    },
    events::{EventManager, SlotFilled, SlotVacated},
    models::{AssertExists, Draft, FindModelResult, Model, User, Role},
};

/// Abstract representation of roles a user can take during an editing process.
#[derive(Debug)]
pub struct Slot {
    data: db::EditProcessSlot,
}

/// A subset of slot's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    pub id: i32,
    pub name: String,
    pub roles: Vec<i32>,
}

impl Model for Slot {
    const ERROR_CATEGORY: &'static str = "edit-process:slot";

    type Id = i32;
    type Database = db::EditProcessSlot;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, id: Self::Id) -> FindModelResult<Self> {
        edit_process_slots::table
            .filter(edit_process_slots::id.eq(id))
            .get_result::<db::EditProcessSlot>(db)
            .map(Slot::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Slot { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Public {
        let db::EditProcessSlot { id, ref name, .. } = self.data;

        Public {
            id,
            name: name.clone(),
            roles: Vec::new(),
        }
    }

    fn get_public_full(&self, db: &Connection, _: &()) -> Result<Public, DbError> {
        let db::EditProcessSlot { id, ref name, .. } = self.data;

        Ok(Public {
            id,
            name: name.clone(),
            roles: self.get_role_limit(db)?,
        })
    }
}

impl Slot {
    /// Get list of all currently unoccupied slots to which a user with given
    /// role can assign themselves.
    pub fn all_free(db: &Connection, user: &User)
    -> Result<Vec<(Draft, Slot)>, DbError> {
        Ok(drafts::table
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
            .inner_join(edit_process_steps::table)
            .inner_join(edit_process_versions::table
                .on(edit_process_steps::process.eq(edit_process_versions::id)))
            .inner_join(edit_processes::table
                .on(edit_process_versions::process.eq(edit_processes::id)))
            .inner_join(team_members::table
                .on(edit_processes::team.eq(team_members::team)))
            .select((
                drafts::all_columns,
                documents::all_columns,
                edit_process_slots::all_columns,
            ))
            .filter(draft_slots::user.is_null()
                .and(edit_process_slot_roles::role.is_null()
                    .or(edit_process_slot_roles::role.nullable().eq(team_members::role)))
                .and(team_members::user.eq(user.id())))
            .get_results::<(
                db::Draft,
                db::Document,
                db::EditProcessSlot,
            )>(db)?
            .into_iter()
            .map(|(draft, document, slot)| (
                Draft::from_db((draft, document)),
                Slot::from_db(slot),
            ))
            .collect())
    }

    /// Does a draft have any unoccupied slots to which a user can assign
    /// themselves?
    pub fn free_in_draft_for(db: &Connection, draft: &Draft)
    -> Result<bool, DbError> {
        // TODO: fold this and all_free into a single helper function?
        Ok(drafts::table
            .inner_join(edit_process_step_slots::table
                .on(drafts::step.eq(edit_process_step_slots::step)))
            .left_join(draft_slots::table
                .on(drafts::module.eq(draft_slots::draft)
                    .and(edit_process_step_slots::slot.eq(draft_slots::slot))))
            .inner_join(edit_process_slots::table
                .on(edit_process_step_slots::slot.eq(edit_process_slots::id)))
            .left_join(edit_process_slot_roles::table
                .on(edit_process_slots::id.eq(edit_process_slot_roles::slot)))
            .inner_join(edit_process_steps::table)
            .inner_join(edit_process_versions::table
                .on(edit_process_steps::process.eq(edit_process_versions::id)))
            .inner_join(edit_processes::table
                .on(edit_process_versions::process.eq(edit_processes::id)))
            .inner_join(team_members::table
                .on(edit_processes::team.eq(team_members::team)))
            .filter(draft_slots::user.is_null()
                .and(edit_process_slot_roles::role.is_null()
                    .or(edit_process_slot_roles::role.nullable().eq(team_members::role)))
                .and(drafts::module.eq(draft.id())))
            .count()
            .get_result::<i64>(db)?
            > 0)
    }

    /// Get current occupant of this slot in a draft.
    pub fn get_occupant(&self, db: &Connection, draft: &Draft)
    -> Result<Option<User>, DbError> {
        draft_slots::table
            .filter(draft_slots::draft.eq(draft.id())
                .and(draft_slots::slot.eq(self.data.id)))
            .get_result::<db::DraftSlot>(db)
            .optional()?
            .map(|slot| User::by_id(db, slot.user).assert_exists())
            .transpose()
    }

    /// Get list of roles to which this slot is limited.
    ///
    /// If the list is empty the there is no limit on this slot.
    pub fn get_role_limit(&self, db: &Connection) -> Result<Vec<i32>, DbError> {
        edit_process_slot_roles::table
            .select(edit_process_slot_roles::role)
            .filter(edit_process_slot_roles::slot.eq(self.data.id))
            .get_results::<i32>(db)
    }

    /// Is this slot limited to only users with a specific role?
    pub fn is_role_limited(&self, db: &Connection) -> Result<bool, DbError> {
        diesel::select(exists(
            edit_process_slot_roles::table
                .filter(edit_process_slot_roles::slot.eq(self.data.id))
        )).get_result(db)
    }

    /// Does the role limit allow a user to occupy this slot?
    pub fn is_allowed_to_occupy(&self, db: &Connection, user: &db::User)
    -> Result<bool, DbError> {
        if !self.is_role_limited(db)? {
            return Ok(true);
        }

        diesel::select(exists(
            team_members::table
                .inner_join(edit_process_slot_roles::table
                    .on(team_members::role.eq(edit_process_slot_roles::role.nullable())))
                .filter(edit_process_slot_roles::slot.eq(self.data.id)
                    .and(team_members::user.eq(user.id)))
        )).get_result(db)
    }

    /// Fill this slot with an auto-selected user for a particular draft.
    pub fn fill(&self, db: &Connection, draft: &Draft)
    -> Result<Option<i32>, FillSlotError> {
        if !self.data.autofill {
            return Ok(None);
        }

        let user = users::table
            .inner_join(team_members::table)
            .inner_join(edit_process_slot_roles::table
                .on(team_members::role.eq(edit_process_slot_roles::role.nullable())))
            .inner_join(draft_slots::table)
            .select(users::all_columns)
            .group_by((users::all_columns, draft_slots::draft))
            .order_by(count_distinct(draft_slots::draft).asc())
            .first::<db::User>(db)
            .optional()?
            .ok_or(FillSlotError::NoUser)?;

        self.fill_with(db, draft, &user)?;

        Ok(Some(user.id))
    }

    /// Fill this slot with a user for a particular draft.
    pub fn fill_with(&self, db: &Connection, draft: &Draft, user: &db::User)
    -> Result<(), FillSlotError> {
        if !self.is_allowed_to_occupy(db, user)? {
            return Err(FillSlotError::BadRole);
        }

        debug!("Assigning {:?} to {:?}", user, self.data);

        let old = draft_slots::table
            .filter(draft_slots::draft.eq(draft.id())
                .and(draft_slots::slot.eq(self.data.id)))
            .select(draft_slots::user.nullable())
            .get_result::<Option<i32>>(db)
            .optional()?
            .unwrap_or(None);

        diesel::insert_into(draft_slots::table)
            .values(db::DraftSlot {
                draft: draft.id(),
                slot: self.data.id,
                user: user.id,
            })
            .on_conflict((draft_slots::draft, draft_slots::slot))
            .do_update()
            .set(draft_slots::user.eq(user.id))
            .execute(db)?;

        EventManager::notify(user, SlotFilled {
            slot: self.data.id,
            module: draft.id(),
            document: draft.id,
        });

        if let Some(old) = old {
            EventManager::notify(old, SlotVacated {
                slot: self.data.id,
                module: draft.id(),
                document: draft.id,
            });
        }

        audit::log_db(db, "drafts", draft.id(), "fill-slot", LogFill {
            slot: self.data.id,
            user: user.id,
        });

        Ok(())
    }

    /// Set slot's name.
    pub fn set_name(&mut self, db: &Connection, name: &str)
    -> Result<(), RenameSlotError> {
        db.transaction(|| {
            audit::log_db(db, "slots", self.id, "set-name", name);

            self.data = diesel::update(&self.data)
                .set(edit_process_slots::name.eq(name))
                .get_result(db)?;

            Ok(())
        })
    }

    /// Set slot's role limit.
    pub fn set_role_limit(&mut self, db: &Connection, roles: &[Role])
    -> Result<(), DbError> {
    let roles = roles.iter().map(|r| r.id).collect::<Vec<_>>();

        db.transaction(|| {
            audit::log_db(db, "slots", self.id, "set-roles", &roles);

            diesel::delete(edit_process_slot_roles::table
                .filter(edit_process_slot_roles::slot.eq(self.data.id))
            ).execute(db)?;

            diesel::insert_into(edit_process_slot_roles::table)
                .values(roles.iter().map(|&role| db::EditProcessSlotRole {
                    slot: self.data.id,
                    role,
                }).collect::<Vec<_>>())
                .execute(db)?;

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

#[derive(ApiError, Debug, Fail, From)]
pub enum FillSlotError {
    /// Database error
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// There is no user available to fill this slot
    #[fail(display = "There is no user available to fill this slot")]
    #[api(code = "edit-process:slot:fill")]
    NoUser,
    /// User doesn't have required role.
    #[api(code = "edit-process:slot:fill:bad-role", status = "BAD_REQUEST")]
    #[fail(display = "User doesn't have required role")]
    BadRole,
}

#[derive(ApiError, Debug, Fail)]
pub enum RenameSlotError {
    #[api(internal)]
    #[fail(display = "{}", _0)]
    Database(#[cause] DbError),
    #[api(code = "edit-process:slot:name:duplicate", status = "BAD_REQUEST")]
    #[fail(display = "rename would result in a duplicate name")]
    DuplicateName,
}

impl From<DbError> for RenameSlotError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) =>
                Self::DuplicateName,
            _ => Self::Database(err),
        }
    }
}


#[derive(Serialize)]
struct LogFill {
    slot: i32,
    user: i32,
}
