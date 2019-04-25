use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};
use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::{
        book_parts,
        document_files,
        documents,
        draft_slots,
        drafts,
        edit_process_step_slots,
    },
    types::SlotPermission,
};
use super::{
    File,
    document::{Document, PublicData as DocumentData},
    editing::{Step, StepData},
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
    pub permissions: Vec<SlotPermission>,
    pub step: StepData,
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

    /// Find draft by ID.
    pub fn by_id(dbconn: &Connection, module: Uuid, user: i32) -> Result<Draft, DbError> {
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
    }

    /// Delete this draft.
    pub fn delete(self, dbconn: &Connection) -> Result<(), DbError> {
        dbconn.transaction(|| {
            diesel::delete(&self.data).execute(dbconn)?;
            self.document.delete(dbconn)?;
            Ok(())
        })
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
            permissions: self.get_permissions(dbconn, user)?,
            step: self.get_step(dbconn)?
                .get_public(dbconn, Some((self.data.module, user)))?,
        })
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

    /// Write into a file in this draft.
    ///
    /// If there already is a file with this name it will be updated, otherwise
    /// a new file will be created.
    pub fn write_file(&self, dbconn: &Connection, name: &str, file: &File)
    -> Result<(), DbError> {
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
    }

    /// Delete a file from this draft.
    pub fn delete_file(&self, dbconn: &Connection, name: &str) -> Result<(), DbError> {
        diesel::delete(document_files::table
            .filter(document_files::document.eq(self.document.id)
                .and(document_files::name.eq(name))))
            .execute(dbconn)?;
        Ok(())
    }

    /// Change title of this draft's document.
    pub fn set_title(&mut self, dbconn: &Connection, title: &str) -> Result<(), DbError> {
        self.document.set_title(dbconn, title)
    }
}

impl std::ops::Deref for Draft {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.document
    }
}
