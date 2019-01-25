use diesel::{
    Connection as _Connection,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use uuid::Uuid;

use crate::db::{
    Connection,
    functions::duplicate_document,
    models as db,
    schema::{documents, drafts, modules, xref_targets},
};
use super::{Document, Draft, File, XrefTarget};

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
    pub title: String,
    pub assignee: Option<i32>,
}

impl Module {
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
                    .map(|(data, document)| Module {
                        data,
                        document: Document::from_db(document),
                    })
                    .collect()
            })
    }

    /// Get all modules assigned to a user.
    pub fn assigned_to(dbconn: &Connection, user: i32) -> Result<Vec<Module>, DbError> {
        modules::table
            .filter(modules::assignee.eq(user))
            .inner_join(documents::table)
            .get_results::<(db::Module, db::Document)>(dbconn)
            .map(|v| {
                v.into_iter()
                    .map(|(data, document)| Module {
                        data,
                        document: Document::from_db(document),
                    })
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
            .map(|(data, document)| Module {
                data,
                document: Document::from_db(document),
            })
    }

    /// Create a new module.
    pub fn create<N, I>(
        dbconn: &Connection,
        title: &str,
        index: File,
        files: I,
    ) -> Result<Module, DbError>
    where
        I: IntoIterator<Item = (N, File)>,
        N: AsRef<str>,
    {
        dbconn.transaction(|| {
            let document = Document::create(dbconn, title, index, files)?;

            let data = diesel::insert_into(modules::table)
                .values(&db::Module {
                    id: Uuid::new_v4(),
                    document: document.id,
                    assignee: None,
                })
                .get_result::<db::Module>(dbconn)?;

            Ok(Module { data, document })
        })
    }

    /// Get ID of this module.
    ///
    /// Since `Module` derefs to [`Document`], `module.id` will return ID of the
    /// this module's current document.
    pub fn id(&self) -> Uuid {
        self.data.id
    }

    /// Get the public portion of this module's data.
    pub fn get_public(&self) -> PublicData {
        PublicData {
            id: self.data.id,
            title: self.document.title.clone(),
            assignee: self.data.assignee,
        }
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

    /// Create a new draft of this module for a given user.
    pub fn create_draft(&self, dbconn: &Connection, user: i32)
    -> Result<Draft, CreateDraftError> {
        if self.data.assignee != Some(user) {
            return Err(CreateDraftError::UserNotAssigned);
        }

        let draft = diesel::insert_into(drafts::table)
            .values((
                drafts::module.eq(self.data.id),
                drafts::user.eq(user),
                drafts::document.eq(duplicate_document(self.document.id)),
            ))
            .get_result::<db::Draft>(dbconn)?;

        let document = documents::table
            .filter(documents::id.eq(draft.document))
            .get_result::<db::Document>(dbconn)?;

        Ok(Draft::from_db(draft, Document::from_db(document)))
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

            let document = Document::create(dbconn, &self.title, index, files)?;

            diesel::update(modules::table.filter(modules::id.eq(self.data.id)))
                .set(modules::document.eq(document.id))
                .execute(dbconn)?;

            self.document = document;

            Ok(())
        })
    }

    /// Change user assigned to this module.
    pub fn set_assignee(&self, dbconn: &Connection, user: Option<i32>)
    -> Result<(), DbError> {
        diesel::update(&self.data)
            .set(modules::assignee.eq(user))
            .execute(dbconn)?;
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
pub enum CreateDraftError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// Tried to create draft for an user other than the one assigned.
    #[fail(display = "Only assigned user can create a draft")]
    #[api(code = "draft:create:not-assigned", status = "BAD_REQUEST")]
    UserNotAssigned,
    /// User already has a draft of this module.
    #[fail(display = "User already has a draft")]
    #[api(code = "draft:create:exists", status = "BAD_REQUEST")]
    Exists,
}

impl_from! { for CreateDraftError ;
    DbError => |e| match e {
        DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) =>
            CreateDraftError::Exists,
        _ => CreateDraftError::Database(e),
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
