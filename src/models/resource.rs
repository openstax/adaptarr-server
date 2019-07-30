use adaptarr_macros::From;
use diesel::{
    Connection as _,
    prelude::*,
    result::{Error as DbError, DatabaseErrorKind},
};
use failure::Fail;
use uuid::Uuid;
use serde::Serialize;

use crate::{
    ApiError,
    audit,
    db::{
        Connection,
        models as db,
        schema::resources,
    },
    models::file::{File, FindFileError},
};

#[derive(Debug)]
pub struct Resource {
    data: db::Resource,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublicData {
    pub id: Uuid,
    pub name: String,
    pub parent: Option<Uuid>,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    File,
    Directory,
}

impl Resource {
    /// Construct `Resource` from its database counterpart.
    pub(super) fn from_db(data: db::Resource) -> Resource {
        Resource { data }
    }

    /// Get all resources.
    pub fn all(db: &Connection) -> Result<Vec<Resource>, DbError> {
        resources::table
            .get_results::<db::Resource>(db)
            .map(|v| v.into_iter().map(Resource::from_db).collect::<Vec<_>>())
    }

    /// Find a resource by ID.
    pub fn by_id(db: &Connection, id: Uuid)
    -> Result<Resource, FindResourceError> {
        resources::table
            .filter(resources::id.eq(id))
            .get_result::<db::Resource>(db)
            .optional()?
            .ok_or(FindResourceError::NotFound)
            .map(Resource::from_db)
    }

    /// Create a new resource.
    pub fn create(
        db: &Connection,
        name: &str,
        file: Option<&File>,
        parent: Option<&Resource>,
    ) -> Result<Resource, CreateResourceError> {
        db.transaction(|| {
            let data = diesel::insert_into(resources::table)
                .values(db::NewResource {
                    id: Uuid::new_v4(),
                    name,
                    file: file.map(|f| f.id),
                    parent: parent.map(|r| r.id),
                })
                .get_result::<db::Resource>(db)?;

            audit::log_db(db, "resources", data.id, "create", LogCreation {
                name,
                file: data.file,
                parent: data.parent,
            });

            Ok(Resource::from_db(data))
        })
    }

    /// Is this file a directory?
    pub fn is_directory(&self) -> bool {
        self.data.file.is_none()
    }

    /// Get the public portion of this resource's data.
    pub fn get_public(&self) -> PublicData {
        let db::Resource { id, ref name, parent, .. } = self.data;

        PublicData {
            id, parent,
            name: name.clone(),
            kind: if self.is_directory() { Kind::Directory } else { Kind::File },
        }
    }

    /// Get the file associated with this resource.
    pub fn get_file(&self, db: &Connection) -> Result<File, FileError> {
        let id = self.data.file.ok_or(FileError::IsADirectory)?;

        File::by_id(db, id)
            .map_err(|e| match e {
                FindFileError::Database(e) => FileError::Database(e),
                FindFileError::NotFound => panic!(
                    "Inconsistent database: missing file for index.cnxml"),
            })
    }

    /// Set resource's name.
    pub fn set_name(&mut self, db: &Connection, name: &str) -> Result<(), DbError> {
        db.transaction(|| {
            audit::log_db(db, "resources", self.data.id, "set-name", name);

            self.data = diesel::update(&self.data)
                .set(resources::name.eq(name))
                .get_result(db)?;

            Ok(())
        })
    }

    /// Set the file associated with this resource.
    pub fn set_file(&mut self, db: &Connection, file: &File)
    -> Result<(), FileError> {
        if self.is_directory() {
            return Err(FileError::IsADirectory);
        }

        db.transaction(|| {
            audit::log_db(db, "resources", self.data.id, "set-file", file.id);

            diesel::update(&self.data)
                .set(resources::file.eq(file.id))
                .execute(db)?;

            self.data.file = Some(file.id);

            Ok(())
        })
    }
}

impl std::ops::Deref for Resource {
    type Target = db::Resource;

    fn deref(&self) -> &db::Resource {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum FindResourceError {
    /// Database error.
    #[fail(display = "database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// No resource found matching given criteria.
    #[fail(display = "no such resource")]
    #[api(code = "resource:not-found", status = "NOT_FOUND")]
    NotFound,
}

#[derive(ApiError, Debug, Fail)]
pub enum CreateResourceError {
    /// Database error.
    #[fail(display = "database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// Duplicate resource.
    #[fail(display = "duplicate resource")]
    #[api(code = "resource:new:exists", status = "BAD_REQUEST")]
    Duplicate,
}

impl From<DbError> for CreateResourceError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)
                => CreateResourceError::Duplicate,
            _ => CreateResourceError::Database(e),
        }
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum FileError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// Resource is a directory.
    #[fail(display = "No such file")]
    #[api(code = "resource:is-a-directory", status = "BAD_REQUEST")]
    IsADirectory,
}

#[derive(Serialize)]
struct LogCreation<'a> {
    name: &'a str,
    file: Option<i32>,
    parent: Option<Uuid>,
}
