use adaptarr_error::ApiError;
use diesel::{
    Connection as _,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use failure::Fail;
use serde::Serialize;

use crate::{
    audit,
    db::{
        Connection,
        models as db,
        schema::roles,
    },
    permissions::PermissionBits,
};
use super::{FindModelError, FindModelResult, Model};

/// Role a user can take.
#[derive(Clone, Debug)]
pub struct Role {
    data: db::Role,
}

/// A subset of role's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    id: i32,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<PermissionBits>,
}

impl Model for Role {
    const ERROR_CATEGORY: &'static str = "role";

    type Id = i32;
    type Database = db::Role;
    type Public = Public;
    type PublicParams = bool;

    fn by_id(db: &Connection, id: Self::Id)
    -> FindModelResult<Self> {
        roles::table
            .filter(roles::id.eq(id))
            .get_result::<db::Role>(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Role { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Public {
        let db::Role { id, ref name, .. } = self.data;

        Public {
            id,
            name: name.clone(),
            permissions: None,
        }
    }

    fn get_public_full(&self, _: &Connection, sensitive: bool)
    -> Result<Public, DbError> {
        let db::Role { id, ref name, .. } = self.data;

        Ok(Public {
            id,
            name: name.clone(),
            permissions: if sensitive { Some(self.permissions()) } else { None },
        })
    }
}

impl Role {
    /// Get all roles.
    pub fn all(db: &Connection) -> Result<Vec<Role>, DbError> {
        roles::table
            .get_results::<db::Role>(db)
            .map(|v| v.into_iter().map(|data| Role { data }).collect())
    }

    /// Find all roles by ID.
    pub fn by_ids(db: &Connection, ids: &[i32])
    -> Result<Vec<Role>, FindModelError<Role>> {
        Ok(roles::table
            .filter(roles::id.eq_any(ids))
            .get_results::<db::Role>(db)?
            .into_iter()
            .map(Role::from_db)
            .collect())
    }

    /// Create a new role.
    pub fn create(db: &Connection, name: &str, permissions: PermissionBits)
    -> Result<Role, CreateRoleError> {
        let data = diesel::insert_into(roles::table)
            .values(db::NewRole {
                name,
                permissions: permissions.bits(),
            })
            .get_result::<db::Role>(db)?;

        audit::log_db(db, "roles", data.id, "create", LogNewRole {
            name,
            permissions: permissions.bits(),
        });

        Ok(Role { data })
    }

    /// Delete this role.
    ///
    /// This will delete only this role. If there are any users assigned it,
    /// they will be unassigned first.
    pub fn delete(self, db: &Connection) -> Result<(), DeleteRoleError> {
        db.transaction(|| {
            diesel::delete(&self.data).execute(db)?;
            audit::log_db(db, "roles", self.data.id, "delete", ());
            Ok(())
        })
    }

    /// Get all permissions this role has.
    pub fn permissions(&self) -> PermissionBits {
        PermissionBits::from_bits_truncate(self.data.permissions)
    }

    /// Set this role's name.
    pub fn set_name(&mut self, db: &Connection, name: &str)
    -> Result<(), DbError> {
        let data = diesel::update(&self.data)
            .set(roles::name.eq(name))
            .get_result::<db::Role>(db)?;

        audit::log_db(db, "roles", self.id, "set-name", name);

        self.data = data;

        Ok(())
    }

    /// Set this role's permissions.
    pub fn set_permissions(
        &mut self,
        db: &Connection,
        permissions: PermissionBits,
    ) -> Result<(), DbError> {
        let data = diesel::update(&self.data)
            .set(roles::permissions.eq(permissions.bits()))
            .get_result::<db::Role>(db)?;

        audit::log_db(
            db, "roles", self.id, "set-permissions", permissions.bits());

        self.data = data;

        Ok(())
    }
}

impl std::ops::Deref for Role {
    type Target = db::Role;

    fn deref(&self) -> &db::Role {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum CreateRoleError {
    /// Creation failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// Duplicate role.
    #[fail(display = "Duplicate role")]
    #[api(code = "role:new:exists", status = "BAD_REQUEST")]
    Duplicate,
}

impl From<DbError> for CreateRoleError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)
                => CreateRoleError::Duplicate,
            _ => CreateRoleError::Database(e),
        }
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum DeleteRoleError {
    #[api(internal)]
    #[fail(display = "{}", _0)]
    Database(#[cause] DbError),
    #[api(code = "role:delete:in-use", status = "BAD_REQUEST")]
    #[fail(display = "role is still in use")]
    InUse,
}

impl From<DbError> for DeleteRoleError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _) =>
                DeleteRoleError::InUse,
            _ => DeleteRoleError::Database(e),
        }
    }
}

#[derive(Serialize)]
struct LogNewRole<'a> {
    name: &'a str,
    // XXX: we serialize permissions as bits as rmp-serde currently works as
    // a human-readable format, and serializes PermissionBits as an array of
    // strings.
    permissions: i32,
}
