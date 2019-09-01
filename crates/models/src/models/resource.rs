use adaptarr_error::ApiError;
use adaptarr_macros::From;
use diesel::{Connection as _, prelude::*, result::{Error as DbError, DatabaseErrorKind}};
use failure::Fail;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    audit,
    db::{
        Connection,
        models as db,
        schema::resources,
    },
};
use super::{AssertExists, FindModelResult, File, Model};

#[derive(Debug)]
pub struct Resource {
    data: db::Resource,
}

#[derive(Clone, Debug, Serialize)]
pub struct Public {
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

impl Model for Resource {
    const ERROR_CATEGORY: &'static str = "resource";

    type Id = Uuid;
    type Database = db::Resource;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, id: Self::Id) -> FindModelResult<Self> {
        resources::table
            .filter(resources::id.eq(id))
            .get_result::<db::Resource>(db)
            .map(Resource::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Resource { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Self::Public {
        let db::Resource { id, ref name, parent, .. } = self.data;

        Public {
            id, parent,
            name: name.clone(),
            kind: if self.is_directory() { Kind::Directory } else { Kind::File },
        }
    }
}

impl Resource {
    /// Get all resources.
    pub fn all(db: &Connection) -> Result<Vec<Resource>, DbError> {
        resources::table
            .get_results::<db::Resource>(db)
            .map(|v| v.into_iter().map(Resource::from_db).collect::<Vec<_>>())
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

    /// Get the file associated with this resource.
    pub fn get_file(&self, db: &Connection) -> Result<File, ResourceFileError> {
        let id = self.data.file.ok_or(ResourceFileError::IsADirectory)?;

        File::by_id(db, id).assert_exists().map_err(From::from)
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
    -> Result<(), ResourceFileError> {
        if self.is_directory() {
            return Err(ResourceFileError::IsADirectory);
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
pub enum ResourceFileError {
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
