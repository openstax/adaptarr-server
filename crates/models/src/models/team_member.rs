use adaptarr_error::ApiError;
use adaptarr_macros::From;
use diesel::{Connection as _, prelude::*, result::Error as DbError};
use failure::Fail;
use serde::Serialize;

use crate::{
    audit,
    db::{Connection, models as db, schema::{roles, team_members}},
    permissions::TeamPermissions,
};
use super::{AssertExists, Model, User, Role, FindModelResult, TeamResource};

pub struct TeamMember {
    data: db::TeamMember,
    role: Option<Role>,
}

#[derive(Serialize)]
pub struct Public {
    user: i32,
    permissions: TeamPermissions,
    role: Option<<Role as Model>::Public>,
}

impl Model for TeamMember {
    const ERROR_CATEGORY: &'static str = "team-member";

    type Id = (i32, i32);
    type Database = (db::TeamMember, Option<db::Role>);
    type Public = Public;
    type PublicParams = bool;

    fn by_id(db: &Connection, (team_id, user_id): Self::Id)
    -> FindModelResult<Self> {
        team_members::table
            .filter(team_members::team.eq(team_id)
                .and(team_members::user.eq(user_id)))
            .left_join(roles::table)
            .get_result(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db((data, role): Self::Database) -> Self {
        TeamMember {
            data,
            role: Model::from_db(role),
        }
    }

    fn into_db(self) -> Self::Database {
        (self.data, self.role.into_db())
    }

    fn id(&self) -> Self::Id {
        (self.data.team, self.data.user)
    }

    fn get_public(&self) -> Self::Public {
        Public {
            user: self.data.user,
            permissions: self.permissions(),
            role: self.role.get_public(),
        }
    }

    fn get_public_full(&self, db: &Connection, sensitive: &bool)
    -> Result<Self::Public, DbError> {
        Ok(Public {
            user: self.data.user,
            permissions: self.permissions(),
            role: self.role.get_public_full(db, sensitive)?,
        })
    }
}

impl TeamMember {
    /// Remove this member from the team.
    pub fn delete(self, db: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(db)?;
        Ok(())
    }

    /// Get all permissions this team member has.
    pub fn permissions(&self) -> TeamPermissions {
        let role = self.role.as_ref()
            .map(Role::permissions)
            .unwrap_or_else(TeamPermissions::empty);

        TeamPermissions::from_bits_truncate(self.data.permissions) | role
    }

    pub fn get_user(&self, db: &Connection) -> Result<User, DbError> {
        User::by_id(db, self.data.user).assert_exists()
    }

    /// Change this member's inherent permissions.
    pub fn set_permissions(
        &mut self,
        db: &Connection,
        permissions: TeamPermissions,
    ) -> Result<(), DbError> {
        db.transaction(|| {
            audit::log_db(
                db,
                "teams",
                self.data.team,
                "set-member-permissions",
                LogSetPermissions {
                    member: self.data.user,
                    permissions: permissions.bits(),
                },
            );

            self.data = diesel::update(&self.data)
                .set(team_members::permissions.eq(permissions.bits()))
                .get_result(db)?;

            Ok(())
        })
    }

    /// Change this member's role.
    pub fn set_role(&mut self, db: &Connection, role: Option<Role>)
    -> Result<(), SetRoleError> {
        if role.as_ref().map(TeamResource::team_id).unwrap_or(self.data.team)
        != self.data.team {
            return Err(SetRoleError::BadRole);
        }

        db.transaction(|| {
            audit::log_db(
                db,
                "teams",
                self.data.team,
                "set-member-role",
                LogSetRole {
                    member: self.data.user,
                    role: role.as_ref().map(Model::id),
                },
            );

            self.data = diesel::update(&self.data)
                .set(team_members::role.eq(role.as_ref().map(Model::id)))
                .get_result(db)?;

            self.role = role;

            Ok(())
        })
    }
}

impl std::ops::Deref for TeamMember {
    type Target = db::TeamMember;

    fn deref(&self) -> &db::TeamMember {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum SetRoleError {
    #[api(internal)]
    #[fail(display = "{}", _0)]
    Database(#[cause] #[from] DbError),
    #[api(code = "role:different-team", status = "BAD_REQUEST")]
    #[fail(display = "can't use role from another team")]
    BadRole,
}

#[derive(Serialize)]
struct LogSetPermissions {
    member: i32,
    permissions: i32,
}

#[derive(Serialize)]
struct LogSetRole {
    member: i32,
    role: Option<i32>,
}
