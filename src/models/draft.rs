use adaptarr_macros::From;
use actix::SystemService;
use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};
use failure::Fail;
use itertools::Itertools;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    ApiError,
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
    events::{DraftAdvanced, EventManager, ProcessEnded, ProcessCancelled},
    permissions::PermissionBits,
    processing::{ProcessDocument, TargetProcessor},
};
use super::{
    File,
    Module,
    User,
    document::{Document, PublicData as DocumentData},
    editing::{Step, StepData, Slot, Version, step::Seating, slot::FillSlotError},
};

#[derive(Debug)]
pub struct Draft {
    data: db::Draft,
    document: Document,
}

#[derive(Debug, Serialize)]
pub struct PublicData {
    pub module: Uuid,
    #[serde(flatten)]
    pub document: DocumentData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<SlotPermission>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<StepData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub books: Option<Vec<Uuid>>,
}

impl Draft {
    /// Construct `Draft` from its database counterpart.
    pub(super) fn from_db(data: db::Draft, document: Document) -> Draft {
        Draft { data, document }
    }

    /// Get all drafts belonging to a user.
    pub fn all_of(dbconn: &Connection, user: i32) -> Result<Vec<Draft>, DbError> {
        drafts::table
            .filter(drafts::module.eq_any(
                draft_slots::table
                    .select(draft_slots::draft)
                    .filter(draft_slots::user.eq(user))))
            .inner_join(documents::table)
            .get_results::<(db::Draft, db::Document)>(dbconn)
            .map(|v| {
                v.into_iter()
                    .map(|(data, document)| Draft {
                        data,
                        document: Document::from_db(document),
                    })
                    .collect()
            })
    }

    /// Find a draft by ID.
    pub fn by_id(dbconn: &Connection, module: Uuid)
    -> Result<Draft, FindDraftError> {
        drafts::table
            .filter(drafts::module.eq(module))
            .inner_join(documents::table)
            .get_result::<(db::Draft, db::Document)>(dbconn)
            .optional()?
            .ok_or(FindDraftError::NotFound)
            .map(|(data, document)| Draft {
                data,
                document: Document::from_db(document),
            })
    }

    /// Find by ID draft owned by a user.
    pub fn by_id_and_user(dbconn: &Connection, module: Uuid, user: i32)
    -> Result<Draft, FindDraftError> {
        drafts::table
            .filter(drafts::module.eq_any(
                draft_slots::table
                    .select(draft_slots::draft)
                    .filter(draft_slots::draft.eq(module)
                        .and(draft_slots::user.eq(user)))))
            .inner_join(documents::table)
            .get_result::<(db::Draft, db::Document)>(dbconn)
            .map(|(data, document)| Draft {
                data,
                document: Document::from_db(document),
            })
            .optional()?
            .ok_or(FindDraftError::NotFound)
    }

    /// Delete this draft.
    pub fn delete(self, dbconn: &Connection) -> Result<(), DbError> {
        dbconn.transaction(|| {
            let members = draft_slots::table
                .filter(draft_slots::draft.eq(self.data.module))
                .select(draft_slots::user)
                .get_results::<i32>(dbconn)?;

            diesel::delete(&self.data).execute(dbconn)?;
            self.document.delete(dbconn)?;

            audit::log_db(dbconn, "drafts", self.data.module, "delete", ());

            EventManager::notify(members, ProcessCancelled {
                module: self.data.module,
            });

            Ok(())
        })
    }

    /// Get ID of the module this draft was created from.
    pub fn module_id(&self) -> Uuid {
        self.data.module
    }

    /// Get list of permissions a user has to a draft.
    pub fn get_permissions(&self, dbconn: &Connection, user: i32)
    -> Result<Vec<SlotPermission>, DbError> {
        draft_slots::table
            .inner_join(edit_process_step_slots::table
                .on(draft_slots::slot.eq(edit_process_step_slots::slot)))
            .filter(draft_slots::draft.eq(self.data.module)
                .and(draft_slots::user.eq(user))
                .and(edit_process_step_slots::step.eq(self.data.step)))
            .select(edit_process_step_slots::permission)
            .get_results(dbconn)
    }

    /// Get details about the editing process this draft follows.
    pub fn get_process(&self, dbconn: &Connection) -> Result<Version, DbError> {
        self.get_step(dbconn)?.get_process(dbconn)
    }

    /// Get details about current editing step.
    pub fn get_step(&self, dbconn: &Connection) -> Result<Step, DbError> {
        Step::by_id(dbconn, self.data.step)
    }

    /// Get the public portion of this drafts's data.
    pub fn get_public(&self, dbconn: &Connection, user: i32)
    -> Result<PublicData, DbError> {
        Ok(PublicData {
            module: self.data.module,
            document: self.document.get_public(),
            permissions: self.get_permissions(dbconn, user).map(Some)?,
            step: self.get_step(dbconn)?
                .get_public(dbconn, Some(self.data.module), Some(user))
                .map(Some)?,
            books: self.get_books(dbconn).map(Some)?,
        })
    }

    /// Get the public portion of this draft's data, excluding data which would
    /// have to be fetched from the database.
    pub fn get_public_small(&self) -> PublicData {
        PublicData {
            module: self.data.module,
            document: self.document.get_public(),
            permissions: None,
            step: None,
            books: None,
        }
    }

    /// Query list of books containing module this draft was derived from.
    pub fn get_books(&self, dbconn: &Connection) -> Result<Vec<Uuid>, DbError> {
        Ok(book_parts::table
            .filter(book_parts::module.eq(self.data.module))
            .get_results::<db::BookPart>(dbconn)?
            .into_iter()
            .map(|part| part.book)
            .collect())
    }

    /// Check that a user can access a draft.
    pub fn check_access(&self, dbconn: &Connection, user: &User)
    -> Result<bool, DbError> {
        // Process managers have access to all drafts.
        if user.permissions(true).contains(PermissionBits::MANAGE_PROCESS) {
            return Ok(true);
        }

        let member = draft_slots::table
            .select(diesel::dsl::count(draft_slots::user))
            .filter(draft_slots::draft.eq(self.data.module)
                .and(draft_slots::user.eq(user.id)))
            .get_result::<i64>(dbconn)
            .map(|c| c > 0)?;
        if member {
            return Ok(true);
        }

        let could_assign = Slot::free_in_draft_for(dbconn, self, user.role)?;
        Ok(could_assign)
    }

    /// Check that a user currently possesses specified slot permissions.
    pub fn check_permission(
        &self,
        dbconn: &Connection,
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
            .get_result::<i64>(dbconn)
            .map(|c| c > 0)
    }

    /// Write into a file in this draft.
    ///
    /// If there already is a file with this name it will be updated, otherwise
    /// a new file will be created.
    pub fn write_file(&self, dbconn: &Connection, name: &str, file: &File)
    -> Result<(), DbError> {
        dbconn.transaction(|| {
            audit::log_db(
                dbconn, "drafts", self.data.module, "write-file", LogWrite {
                    name,
                    file: file.id,
                });

            if name == "index.cnxml" {
                diesel::update(&*self.document)
                    .set(documents::index.eq(file.id))
                    .execute(dbconn)?;
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
                .execute(dbconn)?;

            Ok(())
        })
    }

    /// Delete a file from this draft.
    pub fn delete_file(&self, dbconn: &Connection, name: &str) -> Result<(), DbError> {
        diesel::delete(document_files::table
            .filter(document_files::document.eq(self.document.id)
                .and(document_files::name.eq(name))))
            .execute(dbconn)?;

        audit::log_db(dbconn, "drafts", self.data.module, "delete-file", name);

        Ok(())
    }

    /// Change title of this draft's document.
    pub fn set_title(&mut self, dbconn: &Connection, title: &str) -> Result<(), DbError> {
        self.document.set_title(dbconn, title)?;
        audit::log_db(dbconn, "drafts", self.data.module, "set-title", title);
        Ok(())
    }

    /// Advance this draft to the next editing step.
    pub fn advance(
        mut self,
        dbconn: &Connection,
        user: i32,
        slot: i32,
        target: i32,
    ) -> Result<AdvanceResult, AdvanceDraftError> {
        dbconn.transaction(|| {
            // First verify that (user, slot) pair exists.

            let slot = draft_slots::table
                .filter(draft_slots::draft.eq(self.data.module)
                    .and(draft_slots::slot.eq(slot)))
                .get_result::<db::DraftSlot>(dbconn)
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
                .get_result::<db::EditProcessLink>(dbconn)
                .optional()?
                .ok_or(AdvanceDraftError::BadLink)?;

            let next = Step::by_id(dbconn, link.to)?;

            // Check whether the target step is a final step. If so, create
            // a new version of this draft's module and delete this draft, thus
            // ending the editing process.

            if next.is_final(dbconn)? {
                let members = draft_slots::table
                    .filter(draft_slots::draft.eq(self.data.module))
                    .select(draft_slots::user)
                    .get_results::<i32>(dbconn)?;

                diesel::update(
                    modules::table
                        .filter(modules::id.eq(self.data.module)))
                    .set(modules::document.eq(self.data.document))
                    .execute(dbconn)?;

                diesel::delete(&self.data).execute(dbconn)?;

                TargetProcessor::from_registry()
                    .do_send(ProcessDocument { document: self.document.clone() });

                let module = modules::table
                    .filter(modules::id.eq(self.data.module))
                    .get_result::<db::Module>(dbconn)?;

                EventManager::notify(members, ProcessEnded {
                    module: self.data.module,
                    version: self.data.document,
                });

                audit::log_db_actor(
                    dbconn, user, "drafts", self.data.module, "finish", LogFinish {
                        link: (link.from, link.to),
                        next: next.id,
                        document: self.data.document,
                    });

                return Ok(AdvanceResult::Finished(
                    Module::from_db(module, self.document)));
            }

            // Otherwise we are advancing normally.

            audit::log_db_actor(
                dbconn, user, "drafts", self.data.module, "advance", LogAdvance {
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
                .get_results::<(db::DraftSlot, db::EditProcessStepSlot)>(dbconn)?
                .into_iter()
                .group_by(|(slot, _)| slot.user);

            // First fill in all empty slots:

            let slots = next.get_slot_seating(dbconn, self.data.module)?;

            for Seating { slot, user: seating, .. } in slots {
                if seating.is_none() {
                    slot.fill(dbconn, &self)
                        .map_err(|e| AdvanceDraftError::FillSlot(slot.id, e))?;
                }
            }

            // And finally update the draft

            self.data = diesel::update(&self.data)
                .set(drafts::step.eq(next.id))
                .get_result(dbconn)?;

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

#[derive(ApiError, Debug, Fail, From)]
pub enum FindDraftError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// No draft found matching given criteria.
    #[fail(display = "No such draft")]
    #[api(code = "draft:not-found", status = "NOT_FOUND")]
    NotFound,
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
