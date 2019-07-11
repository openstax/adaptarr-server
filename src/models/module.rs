use diesel::{
    Connection as _Connection,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use failure::Fail;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    ApiError,
    audit,
    db::{
        Connection,
        functions::duplicate_document,
        models as db,
        schema::{
            book_parts,
            documents,
            draft_slots,
            drafts,
            edit_process_steps,
            edit_process_versions,
            modules,
            xref_targets,
        },
    },
    events::{EventManager, SlotFilled},
    processing::TargetProcessor,
};
use super::{
    Draft,
    File,
    User,
    XrefTarget,
    document::{Document, PublicData as DocumentData},
    editing::{Version, Slot},
};

/// A module is a version of Document that can be part of a Book.
#[derive(Debug)]
pub struct Module {
    data: db::Module,
    document: Document,
}

/// A subset of module's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub id: Uuid,
    #[serde(flatten)]
    pub document: DocumentData,
    pub process: Option<ProcessStatus>,
}

#[derive(Debug, Serialize)]
pub struct ProcessStatus {
    pub process: i32,
    pub version: i32,
    pub step: i32,
}

impl Module {
    /// Construct `Module` from its database counterpart.
    pub(crate) fn from_db(data: db::Module, document: Document) -> Self {
        Module { data, document }
    }

    /// Get all modules.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all(dbconn: &Connection) -> Result<Vec<Module>, DbError> {
        modules::table
            .inner_join(documents::table)
            .get_results::<(db::Module, db::Document)>(dbconn)
            .map(|v| {
                v.into_iter()
                    .map(|(data, document)| Module::from_db(
                        data,
                        Document::from_db(document),
                    ))
                    .collect()
            })
    }

    /// Find a module by ID.
    pub fn by_id(dbconn: &Connection, id: Uuid) -> Result<Module, FindModuleError> {
        modules::table
            .filter(modules::id.eq(id))
            .inner_join(documents::table)
            .get_result::<(db::Module, db::Document)>(dbconn)
            .optional()?
            .ok_or(FindModuleError::NotFound)
            .map(|(data, document)| Module::from_db(
                data,
                Document::from_db(document),
            ))
    }

    /// Create a new module.
    pub fn create<N, I>(
        dbconn: &Connection,
        title: &str,
        language: &str,
        index: File,
        files: I,
    ) -> Result<Module, DbError>
    where
        I: IntoIterator<Item = (N, File)>,
        N: AsRef<str>,
    {
        let module = dbconn.transaction::<_, DbError, _>(|| {
            let document = Document::create(dbconn, title, language, index, files)?;

            let data = diesel::insert_into(modules::table)
                .values(&db::Module {
                    id: Uuid::new_v4(),
                    document: document.id,
                })
                .get_result::<db::Module>(dbconn)?;

            audit::log_db(dbconn, "modules", data.id, "create", LogNewModule {
                title,
                language,
                document: document.id,
            });

            Ok(Module::from_db(data, document))
        })?;

        TargetProcessor::process(module.document.clone());

        Ok(module)
    }

    /// Unpack database data.
    pub fn into_db(self) -> (db::Module, db::Document) {
        (self.data, self.document.into_db())
    }

    /// Get ID of this module.
    ///
    /// Since `Module` derefs to [`Document`], `module.id` will return ID of the
    /// this module's current document.
    pub fn id(&self) -> Uuid {
        self.data.id
    }

    /// Get the public portion of this module's data.
    pub fn get_public(&self, dbconn: &Connection) -> Result<PublicData, DbError> {
        let process = drafts::table
            .inner_join(edit_process_steps::table
                .inner_join(edit_process_versions::table
                    .on(edit_process_steps::process.eq(edit_process_versions::id))))
            .filter(drafts::module.eq(self.data.id))
            .get_result::<(
                db::Draft, (db::EditProcessStep, db::EditProcessVersion),
            )>(dbconn)
            .optional()?
            .map(|(_, (step, version))| ProcessStatus {
                process: version.process,
                version: version.id,
                step: step.id,
            });

        Ok(PublicData {
            id: self.data.id,
            document: self.document.get_public(),
            process,
        })
    }

    /// Query list of books containing this module.
    pub fn get_books(&self, dbconn: &Connection) -> Result<Vec<Uuid>, DbError> {
        Ok(book_parts::table
            .filter(book_parts::module.eq(self.data.id))
            .get_results::<db::BookPart>(dbconn)?
            .into_iter()
            .map(|part| part.book)
            .collect())
    }

    /// Get list of all possible cross-reference targets within this module.
    pub fn xref_targets(&self, dbconn: &Connection)
    -> Result<Vec<XrefTarget>, GetXrefTargetsError> {
        if !self.document.xrefs_ready {
            return Err(GetXrefTargetsError::NotReady);
        }

        xref_targets::table
            .filter(xref_targets::document.eq(self.document.id))
            .get_results::<db::XrefTarget>(dbconn)
            .map_err(Into::into)
            .map(|v| v.into_iter().map(XrefTarget::from_db).collect())
    }

    /// Begin a new editing process for this module.
    pub fn begin_process<S>(
        &self,
        dbconn: &Connection,
        version: &Version,
        slots: S,
    ) -> Result<Draft, BeginProcessError>
    where
        S: IntoIterator<Item = (Slot, User)>,
    {
        dbconn.transaction(|| {
            let slots = slots.into_iter()
                .map(|(slot, user)| {
                    if slot.process != version.id {
                        Err(BeginProcessError::BadSlot(
                            slot.id, version.process().id))
                    } else {
                        Ok(db::DraftSlot {
                            draft: self.data.id,
                            slot: slot.id,
                            user: user.id
                        })
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;

            let draft = diesel::insert_into(drafts::table)
                .values((
                    drafts::module.eq(self.data.id),
                    drafts::document.eq(duplicate_document(self.document.id)),
                    drafts::step.eq(version.start),
                ))
                .get_result::<db::Draft>(dbconn)?;

            diesel::insert_into(draft_slots::table)
                .values(&slots)
                .execute(dbconn)?;

            let document = documents::table
                .filter(documents::id.eq(draft.document))
                .get_result::<db::Document>(dbconn)?;

            audit::log_db(
                dbconn, "modules", self.data.id, "begin-process", version.id);

            for slot in slots {
                EventManager::notify(slot.user, SlotFilled {
                    slot: slot.slot,
                    module: self.data.id,
                    document: document.id,
                });
                audit::log_db(
                    dbconn, "drafts", self.data.id, "fill-slot", LogFill {
                        slot: slot.slot,
                        user: slot.user,
                    });
            }

            Ok(Draft::from_db(draft, Document::from_db(document)))
        })
    }

    /// Replace contents of this module.
    pub fn replace<N, I>(
        &mut self,
        dbconn: &Connection,
        index: File,
        files: I,
    ) -> Result<(), ReplaceModuleError>
    where
        I: IntoIterator<Item = (N, File)>,
        N: AsRef<str>,
    {
        dbconn.transaction(|| {
            let count: i64 = drafts::table
                .filter(drafts::module.eq(self.data.id))
                .count()
                .get_result(dbconn)?;

            if count > 0 {
                return Err(ReplaceModuleError::HasDrafts);
            }

            let document = Document::create(dbconn, &self.title, &self.language, index, files)?;

            diesel::update(modules::table.filter(modules::id.eq(self.data.id)))
                .set(modules::document.eq(document.id))
                .execute(dbconn)?;

            audit::log_db(
                dbconn, "modules", self.data.id, "replace-content", document.id);

            self.document = document;

            Ok(())
        })?;

        TargetProcessor::process(self.document.clone());

        Ok(())
    }
}

impl std::ops::Deref for Module {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.document
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindModuleError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// No module found matching given criteria.
    #[fail(display = "No such module")]
    #[api(code = "module:not-found", status = "NOT_FOUND")]
    NotFound,
}

impl_from! { for FindModuleError ;
    DbError => |e| FindModuleError::Database(e),
}

#[derive(ApiError, Debug, Fail)]
pub enum BeginProcessError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// There is already a draft of this module.
    #[fail(display = "There is already a draft of this module")]
    #[api(code = "draft:create:exists", status = "BAD_REQUEST")]
    Exists,
    /// One of the slots specified was not a part of the process specified.
    #[fail(display = "Slot {} is not part of process {}", _0, _1)]
    #[api(code = "draft:create:bad-slot", status = "BAD_REQUEST")]
    BadSlot(i32, i32),
}

impl_from! { for BeginProcessError ;
    DbError => |e| match e {
        DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) =>
            BeginProcessError::Exists,
        _ => BeginProcessError::Database(e),
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum ReplaceModuleError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// Module has drafts.
    #[fail(display = "Module with drafts cannot be replaced")]
    #[api(code = "module:replace:has-draft", status = "BAD_REQUEST")]
    HasDrafts,
}

impl_from! { for ReplaceModuleError ;
    DbError => |e| ReplaceModuleError::Database(e),
}

#[derive(ApiError, Debug, Fail)]
pub enum GetXrefTargetsError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// List of cross-reference targets is yet to be generated for this module.
    #[fail(display = "List of cross references is not yet ready for this module")]
    #[api(code = "module:xref:not-ready", status = "SERVICE_UNAVAILABLE")]
    NotReady,
}

impl_from! { for GetXrefTargetsError ;
    DbError => |e| GetXrefTargetsError::Database(e),
}

#[derive(Serialize)]
struct LogNewModule<'a> {
    title: &'a str,
    language: &'a str,
    document: i32,
}

#[derive(Serialize)]
struct LogFill {
    slot: i32,
    user: i32,
}
