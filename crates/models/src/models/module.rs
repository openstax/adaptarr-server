use adaptarr_error::ApiError;
use adaptarr_macros::From;
use diesel::{
    Connection as _,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use failure::Fail;
use serde::Serialize;
use uuid::Uuid;

use crate::{
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
};
use super::{
    Document,
    Draft,
    File,
    FindModelResult,
    Model,
    User,
    XrefTarget,
    editing::{Slot, Version},
};

/// A module is a version of Document that can be part of a Book.
#[derive(Debug)]
pub struct Module {
    data: db::Module,
    document: Document,
}

/// A subset of module's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    pub id: Uuid,
    #[serde(flatten)]
    pub document: <Document as Model>::Public,
    pub process: Option<ProcessStatus>,
}

#[derive(Debug, Serialize)]
pub struct ProcessStatus {
    pub process: i32,
    pub version: i32,
    pub step: StepData,
}

#[derive(Debug, Serialize)]
pub struct StepData {
    pub id: i32,
    pub name: String,
}

impl Model for Module {
    const ERROR_CATEGORY: &'static str = "module";

    type Id = Uuid;
    type Database = (db::Module, db::Document);
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, id: Uuid) -> FindModelResult<Module> {
        modules::table
            .filter(modules::id.eq(id))
            .inner_join(documents::table)
            .get_result::<(db::Module, db::Document)>(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db((data, document): Self::Database) -> Self {
        Module {
            data,
            document: Document::from_db(document),
        }
    }

    fn into_db(self) -> Self::Database {
        (self.data, self.document.into_db())
    }

    fn id(&self) -> Uuid {
        self.data.id
    }

    fn get_public(&self) -> Public {
        Public {
            id: self.data.id,
            document: self.document.get_public(),
            process: None,
        }
    }

    fn get_public_full(&self, db: &Connection, _: &()) -> Result<Public, DbError> {
        let process = drafts::table
            .inner_join(edit_process_steps::table
                .inner_join(edit_process_versions::table
                    .on(edit_process_steps::process.eq(edit_process_versions::id))))
            .filter(drafts::module.eq(self.data.id))
            .get_result::<(
                db::Draft, (db::EditProcessStep, db::EditProcessVersion),
            )>(db)
            .optional()?
            .map(|(_, (step, version))| ProcessStatus {
                process: version.process,
                version: version.id,
                step: StepData {
                    id: step.id,
                    name: step.name,
                },
            });

        Ok(Public {
            id: self.data.id,
            document: self.document.get_public_full(db, ())?,
            process,
        })
    }
}

impl Module {
    /// Get all modules.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all(db: &Connection) -> Result<Vec<Module>, DbError> {
        modules::table
            .inner_join(documents::table)
            .get_results::<(db::Module, db::Document)>(db)
            .map(|v| v.into_iter().map(Self::from_db).collect())
    }

    /// Create a new module.
    pub fn create<N, I>(
        db: &Connection,
        title: &str,
        language: &str,
        index: File,
        files: I,
    ) -> Result<Module, DbError>
    where
        I: IntoIterator<Item = (N, File)>,
        N: AsRef<str>,
    {
        let module = db.transaction::<_, DbError, _>(|| {
            let document = Document::create(db, title, language, index, files)?;

            let data = diesel::insert_into(modules::table)
                .values(&db::Module {
                    id: Uuid::new_v4(),
                    document: document.id,
                })
                .get_result::<db::Module>(db)?;

            audit::log_db(db, "modules", data.id, "create", LogNewModule {
                document: document.id,
            });

            Ok(Module { data, document })
        })?;

        // TargetProcessor::process(module.document.clone());

        Ok(module)
    }

    /// Query list of books containing this module.
    pub fn get_books(&self, db: &Connection) -> Result<Vec<Uuid>, DbError> {
        Ok(book_parts::table
            .filter(book_parts::module.eq(self.data.id))
            .get_results::<db::BookPart>(db)?
            .into_iter()
            .map(|part| part.book)
            .collect())
    }

    /// Get list of all possible cross-reference targets within this module.
    pub fn xref_targets(&self, db: &Connection)
    -> Result<Vec<XrefTarget>, GetXrefTargetsError> {
        if !self.document.xrefs_ready {
            return Err(GetXrefTargetsError::NotReady);
        }

        xref_targets::table
            .filter(xref_targets::document.eq(self.document.id))
            .get_results::<db::XrefTarget>(db)
            .map_err(Into::into)
            .map(Model::from_db)
    }

    /// Begin a new editing process for this module.
    pub fn begin_process<S>(
        &self,
        db: &Connection,
        version: &Version,
        slots: S,
    ) -> Result<Draft, BeginProcessError>
    where
        S: IntoIterator<Item = (Slot, User)>,
    {
        db.transaction(|| {
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
                .get_result::<db::Draft>(db)?;

            diesel::insert_into(draft_slots::table)
                .values(&slots)
                .execute(db)?;

            let document = documents::table
                .filter(documents::id.eq(draft.document))
                .get_result::<db::Document>(db)?;

            audit::log_db(
                db, "documents", document.id, "clone-from", self.document.id);

            audit::log_db(
                db, "modules", self.data.id, "begin-process", version.id);

            audit::log_db(db, "drafts", draft.module, "create", LogNewDraft {
                from: self.data.id,
                document: document.id,
            });

            for slot in slots {
                // EventManager::notify(slot.user, SlotFilled {
                //     slot: slot.slot,
                //     module: self.data.id,
                //     document: document.id,
                // });
                audit::log_db(
                    db, "drafts", self.data.id, "fill-slot", LogFill {
                        slot: slot.slot,
                        user: slot.user,
                    });
            }

            Ok(Draft::from_db((draft, document)))
        })
    }

    /// Replace contents of this module.
    pub fn replace<N, I>(
        &mut self,
        db: &Connection,
        index: File,
        files: I,
    ) -> Result<(), ReplaceModuleError>
    where
        I: IntoIterator<Item = (N, File)>,
        N: AsRef<str>,
    {
        db.transaction(|| {
            let count: i64 = drafts::table
                .filter(drafts::module.eq(self.data.id))
                .count()
                .get_result(db)?;

            if count > 0 {
                return Err(ReplaceModuleError::HasDrafts);
            }

            let document = Document::create(db, &self.title, &self.language, index, files)?;

            diesel::update(modules::table.filter(modules::id.eq(self.data.id)))
                .set(modules::document.eq(document.id))
                .execute(db)?;

            audit::log_db(
                db, "modules", self.data.id, "replace-content", document.id);

            self.document = document;

            Ok(())
        })?;

        // TargetProcessor::process(self.document.clone());

        Ok(())
    }
}

impl std::ops::Deref for Module {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.document
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum GetXrefTargetsError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// List of cross-reference targets is yet to be generated for this module.
    #[fail(display = "List of cross references is not yet ready for this module")]
    #[api(code = "module:xref:not-ready", status = "SERVICE_UNAVAILABLE")]
    NotReady,
}

#[derive(ApiError, Debug, Fail, From)]
pub enum ReplaceModuleError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// Module has drafts.
    #[fail(display = "Module with drafts cannot be replaced")]
    #[api(code = "module:replace:has-draft", status = "BAD_REQUEST")]
    HasDrafts,
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

impl From<DbError> for BeginProcessError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) =>
                BeginProcessError::Exists,
            _ => BeginProcessError::Database(e),
        }
    }
}

#[derive(Serialize)]
struct LogNewModule {
    document: i32,
}

#[derive(Serialize)]
struct LogFill {
    slot: i32,
    user: i32,
}

#[derive(Serialize)]
struct LogNewDraft {
    /// Module from which this draft was derived.
    from: Uuid,
    /// Document containing the first version of this draft.
    document: i32,
}
