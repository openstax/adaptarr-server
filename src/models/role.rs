use diesel::{prelude::*, result::{DatabaseErrorKind, Error as DbError}};

use crate::{
    db::{
        Connection,
        models as db,
        schema::roles,
    },
    permissions::PermissionBits,
};

/// Role a user can take.
#[derive(Debug)]
pub struct Role {
    data: db::Role,
}

/// A subset of role's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    id: i32,
    name: String,
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

    /// Create a new role.
    pub fn create(dbcon: &Connection, name: &str, permissions: PermissionBits)
    -> Result<Role, CreateRoleError> {
        diesel::insert_into(roles::table)
            .values(db::NewRole {
                name,
                permissions: permissions.bits(),
            })
            .get_result::<db::Role>(dbcon)
            .map(|data| Role { data })
            .map_err(Into::into)
    }

    /// Delete this role.
    ///
    /// This will delete only this role. If there are any users assigned it,
    /// they will be unassigned first.
    pub fn delete(self, dbcon: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(dbcon)?;
        Ok(())
    }

    /// Get public portion of this role's data.
    pub fn get_public(&self) -> PublicData {
        let db::Role { id, ref name, .. } = self.data;

        PublicData {
            id,
            name: name.clone(),
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
