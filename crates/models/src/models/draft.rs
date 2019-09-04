use actix::SystemService;
use adaptarr_error::ApiError;
use adaptarr_macros::From;
use diesel::{Connection as _, prelude::*, result::Error as DbError};
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
            book_parts,
            document_files,
            documents,
            draft_slots,
            drafts,
            edit_process_links,
            edit_process_step_slots,
            modules,
        },
        types::SlotPermission,
    },
    events::{DraftAdvanced, EventManager, ProcessCancelled, ProcessEnded},
    permissions::PermissionBits,
    processing::{TargetProcessor, ProcessDocument},
};
use super::{
    AssertExists,
    Document,
    File,
    FindModelError,
    FindModelResult,
    Model,
    Module,
    User,
    editing::{FillSlotError, Seating, Slot, Step, Version},
};

#[derive(Debug)]
pub struct Draft {
    data: db::Draft,
    document: Document,
}

#[derive(Debug, Serialize)]
pub struct Public {
    pub module: Uuid,
    #[serde(flatten)]
    pub document: <Document as Model>::Public,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<SlotPermission>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<<Step as Model>::Public>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub books: Option<Vec<Uuid>>,
}

impl Model for Draft {
    const ERROR_CATEGORY: &'static str = "draft";

    type Id = Uuid;
    type Database = (db::Draft, db::Document);
    type Public = Public;
    type PublicParams = i32;

    fn by_id(db: &Connection, id: Self::Id) -> FindModelResult<Self> {
        drafts::table
            .filter(drafts::module.eq(id))
            .inner_join(documents::table)
            .get_result::<(db::Draft, db::Document)>(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db((data, document): Self::Database) -> Self {
        Draft {
            data,
            document: Document::from_db(document),
        }
    }

    fn into_db(self) -> Self::Database {
        (self.data, self.document.into_db())
    }

    fn id(&self) -> Self::Id {
        self.data.module
    }

    fn get_public(&self) -> Public {
        Public {
            module: self.data.module,
            document: self.document.get_public(),
            permissions: None,
            step: None,
            books: None,
        }
    }

    fn get_public_full(&self, db: &Connection, user: i32) -> Result<Public, DbError> {
        Ok(Public {
            module: self.data.module,
            document: self.document.get_public(),
            permissions: self.get_permissions(db, user).map(Some)?,
            step: self.get_step(db)?
                .get_public_full(db, (Some(self.data.module), Some(user)))
                .map(Some)?,
            books: self.get_books(db).map(Some)?,
        })
    }
}

impl Draft {
    /// Get all drafts belonging to a user.
    pub fn all_of(db: &Connection, user: i32) -> Result<Vec<Draft>, DbError> {
        drafts::table
            .filter(drafts::module.eq_any(
                draft_slots::table
                    .select(draft_slots::draft)
                    .filter(draft_slots::user.eq(user))))
            .inner_join(documents::table)
            .get_results::<(db::Draft, db::Document)>(db)
            .map(|v| v.into_iter().map(Self::from_db).collect())
    }

    /// Find by ID draft owned by a user.
    pub fn by_id_and_user(db: &Connection, module: Uuid, user: i32)
    -> FindModelResult<Draft> {
        drafts::table
            .filter(drafts::module.eq_any(
                draft_slots::table
                    .select(draft_slots::draft)
                    .filter(draft_slots::draft.eq(module)
                        .and(draft_slots::user.eq(user)))))
            .inner_join(documents::table)
            .get_result::<(db::Draft, db::Document)>(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    /// Delete this draft.
    pub fn delete(self, db: &Connection) -> Result<(), DbError> {
        db.transaction(|| {
            let members = draft_slots::table
                .filter(draft_slots::draft.eq(self.data.module))
                .select(draft_slots::user)
                .get_results::<i32>(db)?;

            diesel::delete(&self.data).execute(db)?;
            self.document.delete(db)?;

            audit::log_db(db, "drafts", self.data.module, "delete", ());

            EventManager::notify(members, ProcessCancelled {
                module: self.data.module,
            });

            Ok(())
        })
    }

    /// Get list of permissions a user has to a draft.
    pub fn get_permissions(&self, db: &Connection, user: i32)
    -> Result<Vec<SlotPermission>, DbError> {
        draft_slots::table
            .inner_join(edit_process_step_slots::table
                .on(draft_slots::slot.eq(edit_process_step_slots::slot)))
            .filter(draft_slots::draft.eq(self.data.module)
                .and(draft_slots::user.eq(user))
                .and(edit_process_step_slots::step.eq(self.data.step)))
            .select(edit_process_step_slots::permission)
            .get_results(db)
    }

    /// Get details about the editing process this draft follows.
    pub fn get_process(&self, db: &Connection) -> Result<Version, DbError> {
        self.get_step(db)?.get_process(db)
    }

    /// Get details about current editing step.
    pub fn get_step(&self, db: &Connection) -> Result<Step, DbError> {
        Step::by_id(db, self.data.step).assert_exists()
    }

    /// Query list of books containing module this draft was derived from.
    pub fn get_books(&self, db: &Connection) -> Result<Vec<Uuid>, DbError> {
        Ok(book_parts::table
            .filter(book_parts::module.eq(self.data.module))
            .get_results::<db::BookPart>(db)?
            .into_iter()
            .map(|part| part.book)
            .collect())
    }

    /// Check that a user can access a draft.
    pub fn check_access(&self, db: &Connection, user: &User)
    -> Result<bool, DbError> {
        // Process managers have access to all drafts.
        if user.permissions(true).contains(PermissionBits::MANAGE_PROCESS) {
            return Ok(true);
        }

        let member = draft_slots::table
            .select(diesel::dsl::count(draft_slots::user))
            .filter(draft_slots::draft.eq(self.data.module)
                .and(draft_slots::user.eq(user.id)))
            .get_result::<i64>(db)
            .map(|c| c > 0)?;
        if member {
            return Ok(true);
        }

        let could_assign = Slot::free_in_draft_for(db, self, user.role)?;
        Ok(could_assign)
    }

    /// Check that a user currently possesses specified slot permissions.
    pub fn check_permission(
        &self,
        db: &Connection,
        user: i32,
        permission: SlotPermission,
    ) -> Result<bool, DbError> {
        edit_process_step_slots::table
            .inner_join(draft_slots::table
                .on(draft_slots::slot.eq(edit_process_step_slots::slot)))
            .select(diesel::dsl::count(edit_process_step_slots::permission))
            .filter(edit_process_step_slots::step.eq(self.data.step)
                .and(edit_process_step_slots::permission.eq(permission))
                .and(draft_slots::user.eq(user)))
            .get_result::<i64>(db)
            .map(|c| c > 0)
    }

    /// Write into a file in this draft.
    ///
    /// If there already is a file with this name it will be updated, otherwise
    /// a new file will be created.
    pub fn write_file(&self, db: &Connection, name: &str, file: &File)
    -> Result<(), DbError> {
        db.transaction(|| {
            audit::log_db(
                db, "drafts", self.data.module, "write-file", LogWrite {
                    name,
                    file: file.id,
                });

            if name == "index.cnxml" {
                diesel::update(&*self.document)
                    .set(documents::index.eq(file.id))
                    .execute(db)?;
                return Ok(());
            }

            diesel::insert_into(document_files::table)
                .values(&db::NewDocumentFile {
                    document: self.document.id,
                    name,
                    file: file.id,
                })
                .on_conflict((document_files::document, document_files::name))
                .do_update()
                .set(document_files::file.eq(file.id))
                .execute(db)?;

            Ok(())
        })
    }

    /// Delete a file from this draft.
    pub fn delete_file(&self, db: &Connection, name: &str) -> Result<(), DbError> {
        diesel::delete(document_files::table
            .filter(document_files::document.eq(self.document.id)
                .and(document_files::name.eq(name))))
            .execute(db)?;

        audit::log_db(db, "drafts", self.data.module, "delete-file", name);

        Ok(())
    }

    /// Change title of this draft's document.
    pub fn set_title(&mut self, db: &Connection, title: &str) -> Result<(), DbError> {
        self.document.set_title(db, title)?;
        audit::log_db(db, "drafts", self.data.module, "set-title", title);
        Ok(())
    }

    /// Advance this draft to the next editing step.
    pub fn advance(
        mut self,
        db: &Connection,
        user: i32,
        slot: i32,
        target: i32,
    ) -> Result<AdvanceResult, AdvanceDraftError> {
        db.transaction(|| {
            // First verify that (user, slot) pair exists.

            let slot = draft_slots::table
                .filter(draft_slots::draft.eq(self.data.module)
                    .and(draft_slots::slot.eq(slot)))
                .get_result::<db::DraftSlot>(db)
                .optional()?
                .ok_or(AdvanceDraftError::BadSlot)?;

            if slot.user != user {
                return Err(AdvanceDraftError::BadUser);
            }

            // Next verify that (slot, target) link exists

            let link = edit_process_links::table
                .filter(edit_process_links::from.eq(self.data.step)
                    .and(edit_process_links::to.eq(target))
                    .and(edit_process_links::slot.eq(slot.slot)))
                .get_result::<db::EditProcessLink>(db)
                .optional()?
                .ok_or(AdvanceDraftError::BadLink)?;

            let next = match Step::by_id(db, link.to) {
                Ok(step) => step,
                Err(FindModelError::Database(_, err)) =>
                    return Err(AdvanceDraftError::Database(err)),
                Err(FindModelError::NotFound(_)) =>
                    return Err(AdvanceDraftError::BadLink),
            };

            // Check whether the target step is a final step. If so, create
            // a new version of this draft's module and delete this draft, thus
            // ending the editing process.

            if next.is_final(db)? {
                let members = draft_slots::table
                    .filter(draft_slots::draft.eq(self.data.module))
                    .select(draft_slots::user)
                    .get_results::<i32>(db)?;

                diesel::update(
                    modules::table
                        .filter(modules::id.eq(self.data.module)))
                    .set(modules::document.eq(self.data.document))
                    .execute(db)?;

                diesel::delete(&self.data).execute(db)?;

                TargetProcessor::from_registry()
                    .do_send(ProcessDocument { document: self.document.clone() });

                let module = modules::table
                    .filter(modules::id.eq(self.data.module))
                    .get_result::<db::Module>(db)?;

                EventManager::notify(members, ProcessEnded {
                    module: self.data.module,
                    version: self.data.document,
                });

                audit::log_db_actor(
                    db, user, "drafts", self.data.module, "finish", LogFinish {
                        link: (link.from, link.to),
                        next: next.id,
                        document: self.data.document,
                    });

                return Ok(AdvanceResult::Finished(
                    Module::from_db((module, self.document.into_db()))));
            }

            // Otherwise we are advancing normally.

            audit::log_db_actor(
                db, user, "drafts", self.data.module, "advance", LogAdvance {
                    link: (link.from, link.to),
                    next: next.id,
                });

            // Get users' permissions in the next step. We do it before filling
            // slots to avoid sending two notifications to newly assigned users.
            let permissions = draft_slots::table
                .inner_join(edit_process_step_slots::table
                    .on(draft_slots::slot.eq(edit_process_step_slots::slot)))
                .filter(draft_slots::draft.eq(self.data.module)
                    .and(edit_process_step_slots::step.eq(next.id)))
                .order_by(draft_slots::user)
                .get_results::<(db::DraftSlot, db::EditProcessStepSlot)>(db)?
                .into_iter()
                .group_by(|(slot, _)| slot.user);

            // First fill in all empty slots:

            let slots = next.get_slot_seating(db, self.data.module)?;

            for Seating { slot, user: seating, .. } in slots {
                if seating.is_none() {
                    slot.fill(db, &self)
                        .map_err(|e| AdvanceDraftError::FillSlot(slot.id, e))?;
                }
            }

            // And finally update the draft

            self.data = diesel::update(&self.data)
                .set(drafts::step.eq(next.id))
                .get_result(db)?;

            for (user, permissions) in permissions.into_iter() {
                let permissions = permissions
                    .map(|(_, p)| p.permission)
                    .collect();

                EventManager::notify(user, DraftAdvanced {
                    module: self.data.module,
                    document: self.document.id,
                    step: next.id,
                    permissions,
                });
            }

            Ok(AdvanceResult::Advanced(self))
        })
    }
}

impl std::ops::Deref for Draft {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.document
    }
}

pub enum AdvanceResult {
    /// Draft was advanced to the next step.
    Advanced(Draft),
    /// Edit process has finished and resulted in a new version of the module.
    Finished(Module),
}

#[derive(ApiError, Debug, Fail, From)]
pub enum AdvanceDraftError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// Specified slot doesn't exist or has no permissions in current step.
    #[fail(display = "Requested slot doesn't exist in this step")]
    #[api(code = "draft:advance:bad-slot", status = "BAD_REQUEST")]
    BadSlot,
    /// User making request does not occupy the slot they is trying to use.
    #[fail(display = "User doesn't occupy requested slot")]
    #[api(code = "draft:advance:bad-user", status = "FORBIDDEN")]
    BadUser,
    /// Specified link does not exist.
    #[fail(display = "Requested link doesn't exist")]
    #[api(code = "draft:advance:bad-link", status = "BAD_REQUEST")]
    BadLink,
    /// Could not fill a slot,
    #[fail(display = "Could not fill slot {}: {}", _0, _1)]
    FillSlot(i32, #[cause] FillSlotError),
}

#[derive(Serialize)]
struct LogWrite<'a> {
    name: &'a str,
    file: i32,
}

#[derive(Serialize)]
struct LogFinish {
    link: (i32, i32),
    next: i32,
    document: i32,
}

#[derive(Serialize)]
struct LogAdvance {
    link: (i32, i32),
    next: i32,
}
