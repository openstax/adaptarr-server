use diesel::{prelude::*, result::{DatabaseErrorKind, Error as DbError}};
use failure::Fail;
use serde::Serialize;

use crate::{
    ApiError,
    audit,
    db::{
        Connection,
        models as db,
        schema::roles,
    },
    permissions::PermissionBits,
};

/// Role a user can take.
#[derive(Clone, Debug)]
pub struct Role {
    data: db::Role,
}

/// A subset of role's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    id: i32,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<PermissionBits>,
}

impl Role {
    /// Construct `Role` from its database counterpart.
    pub(super) fn from_db(data: db::Role) -> Role {
        Role { data }
    }

    /// Get all roles.
    pub fn all(dbcon: &Connection) -> Result<Vec<Role>, DbError> {
        roles::table
            .get_results::<db::Role>(dbcon)
            .map(|v| v.into_iter().map(|data| Role { data }).collect())
    }

    /// Find a role by ID.
    pub fn by_id(dbcon: &Connection, id: i32) -> Result<Role, FindRoleError> {
        roles::table
            .filter(roles::id.eq(id))
            .get_result::<db::Role>(dbcon)
            .optional()?
            .ok_or(FindRoleError::NotFound)
            .map(|data| Role { data })
    }

    /// Find all roles by ID.
    pub fn by_ids(dbcon: &Connection, ids: &[i32])
    -> Result<Vec<Role>, FindRoleError> {
        Ok(roles::table
            .filter(roles::id.eq_any(ids))
            .get_results::<db::Role>(dbcon)?
            .into_iter()
            .map(Role::from_db)
            .collect())
    }

    /// Create a new role.
    pub fn create(dbcon: &Connection, name: &str, permissions: PermissionBits)
    -> Result<Role, CreateRoleError> {
        let data = diesel::insert_into(roles::table)
            .values(db::NewRole {
                name,
                permissions: permissions.bits(),
            })
            .get_result::<db::Role>(dbcon)?;

        audit::log_db(dbcon, "roles", data.id, "create", LogNewRole {
            name,
            permissions: permissions.bits(),
        });

        Ok(Role { data })
    }

    /// Get underlying database model.
    pub fn into_db(self) -> db::Role {
        self.data
    }

    /// Delete this role.
    ///
    /// This will delete only this role. If there are any users assigned it,
    /// they will be unassigned first.
    pub fn delete(self, dbcon: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(dbcon)?;
        audit::log_db(dbcon, "roles", self.id, "delete", ());
        Ok(())
    }

    /// Get public portion of this role's data.
    pub fn get_public(&self, sensitive: bool) -> PublicData {
        let db::Role { id, ref name, .. } = self.data;

        PublicData {
            id,
            name: name.clone(),
            permissions: if sensitive { Some(self.permissions()) } else { None },
        }
    }

    /// Get all permissions this role has.
    pub fn permissions(&self) -> PermissionBits {
        PermissionBits::from_bits_truncate(self.data.permissions)
    }

    /// Set this role's name.
    pub fn set_name(&mut self, dbcon: &Connection, name: &str)
    -> Result<(), DbError> {
        let data = diesel::update(&self.data)
            .set(roles::name.eq(name))
            .get_result::<db::Role>(dbcon)?;

        audit::log_db(dbcon, "roles", self.id, "set-name", name);

        self.data = data;

        Ok(())
    }

    /// Set this role's permissions.
    pub fn set_permissions(
        &mut self,
        dbcon: &Connection,
        permissions: PermissionBits,
    ) -> Result<(), DbError> {
        let data = diesel::update(&self.data)
            .set(roles::permissions.eq(permissions.bits()))
            .get_result::<db::Role>(dbcon)?;

        audit::log_db(
            dbcon, "roles", self.id, "set-permissions", permissions.bits());

        self.data = data;

        Ok(())
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindRoleError {
    /// Creation failed due to a database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// No role found for given email address.
    #[fail(display = "No such role")]
    #[api(code = "role:not-found", status = "NOT_FOUND")]
    NotFound,
}

impl_from! { for FindRoleError ;
    DbError => |e| FindRoleError::Database(e),
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

impl_from! { for CreateRoleError ;
    DbError => |e| match e {
        DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)
            => CreateRoleError::Duplicate,
        _ => CreateRoleError::Database(e),
    },
}

impl std::ops::Deref for Role {
    type Target = db::Role;

    fn deref(&self) -> &db::Role {
        &self.data
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
