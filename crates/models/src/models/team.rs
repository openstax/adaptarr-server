use adaptarr_error::ApiError;
use adaptarr_macros::From;
use diesel::{Connection as _, prelude::*, result::Error as DbError};
use failure::Fail;
use serde::Serialize;

use crate::{
    audit,
    db::{Connection, models as db, schema::{roles, teams, team_members}},
    permissions::TeamPermissions,
};
use super::{AssertExists, FindModelResult, Model, Role, TeamMember, User};

#[derive(Debug)]
pub struct Team {
    data: db::Team,
}

pub trait TeamResource: Model {
    fn team_id(&self) -> <Team as Model>::Id;

    fn get_team(&self, db: &Connection) -> Result<Team, DbError> {
        Team::by_id(db, self.team_id()).assert_exists()
    }
}

#[derive(Debug, Serialize)]
pub struct Public {
    pub id: i32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<<Role as Model>::Public>>,
}

#[derive(Default)]
pub struct PublicParams {
    /// Include role's permissions in [`Public::roles`].
    pub include_role_permissions: bool,
}

impl TeamResource for Team {
    fn team_id(&self) -> <Team as Model>::Id {
        self.data.id
    }
}

impl Model for Team {
    const ERROR_CATEGORY: &'static str = "team";

    type Id = i32;
    type Database = db::Team;
    type Public = Public;
    type PublicParams = PublicParams;

    fn by_id(db: &Connection, id: Self::Id) -> FindModelResult<Self> {
        teams::table
            .filter(teams::id.eq(id))
            .get_result(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Team { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        self.data.id
    }

    fn get_public(&self) -> Self::Public {
        let db::Team { id, ref name } = self.data;

        Public {
            id,
            name: name.clone(),
            roles: None,
        }
    }

    fn get_public_full(&self, db: &Connection, params: &PublicParams)
    -> Result<Self::Public, DbError> {
        let db::Team { id, ref name } = self.data;

        Ok(Public {
            id,
            name: name.clone(),
            roles: Some(self.get_roles(db)?
                .get_public_full(db, &params.include_role_permissions)?),
        })
    }
}

impl Team {
    /// Get all teams.
    pub fn all(db: &Connection) -> Result<Vec<Team>, DbError> {
        teams::table
            .get_results(db)
            .map(Model::from_db)
    }

    /// Create a new team.
    pub fn create(db: &Connection, name: &str) -> Result<Team, DbError> {
        db.transaction(|| {
            let data = diesel::insert_into(teams::table)
                .values(db::NewTeam {
                    name,
                })
                .get_result::<db::Team>(db)?;

            audit::log_db(db, "teams", data.id, "create", LogNewTeam { name });

            Ok(Model::from_db(data))
        })
    }

    /// Get a role by ID.
    pub fn get_role(&self, db: &Connection, id: <Role as Model>::Id)
    -> FindModelResult<Role> {
        roles::table
            .filter(roles::id.eq(id).and(roles::team.eq(self.data.id)))
            .get_result(db)
            .map(Model::from_db)
            .map_err(From::from)
    }

    /// Get list of all roles in this team.
    pub fn get_roles(&self, db: &Connection) -> Result<Vec<Role>, DbError> {
        roles::table
            .filter(roles::team.eq(self.data.id))
            .get_results(db)
            .map(Model::from_db)
    }

    /// Get membership information for a user.
    pub fn get_member(&self, db: &Connection, user: &User)
    -> FindModelResult<TeamMember> {
        TeamMember::by_id(db, (self.data.id, user.id()))
    }

    /// Get list of all members of this team.
    pub fn get_members(&self, db: &Connection)
    -> Result<Vec<TeamMember>, DbError> {
        team_members::table
            .filter(team_members::team.eq(self.data.id))
            .left_join(roles::table)
            .get_results(db)
            .map(Model::from_db)
    }

    /// Change team's name.
    pub fn set_name(&mut self, db: &Connection, name: &str) -> Result<(), DbError> {
        db.transaction(|| {
            audit::log_db(db, "teams", self.data.id, "set-name", name);

            self.data = diesel::update(&self.data)
                .set(teams::name.eq(name))
                .get_result(db)?;

            Ok(())
        })
    }

    /// Add a new member to this team.
    pub fn add_member(
        &mut self,
        db: &Connection,
        user: &User,
        permissions: TeamPermissions,
        role: Option<&Role>,
    ) -> Result<TeamMember, AddMemberError> {
        if role.map(TeamResource::team_id).unwrap_or(self.data.id) != self.data.id {
            return Err(AddMemberError::BadRole);
        }

        db.transaction(|| {
            audit::log_db(db, "teams", self.data.id, "add-member", LogAddMember {
                user: user.id(),
                permissions: permissions.bits(),
                role: role.map(Model::id),
            });

            let data = diesel::insert_into(team_members::table)
                .values(db::TeamMember {
                    team: self.data.id,
                    user: user.id(),
                    permissions: permissions.bits(),
                    role: role.map(Model::id),
                })
                .get_result(db)?;

            Ok(Model::from_db((data, role.cloned().map(Model::into_db))))
        })
    }
}

impl std::ops::Deref for Team {
    type Target = db::Team;

    fn deref(&self) -> &db::Team {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail, From)]
pub enum AddMemberError {
    #[api(internal)]
    #[fail(display = "{}", _0)]
    Database(#[cause] #[from] DbError),
    #[api(code = "role:different-team", status = "BAD_REQUEST")]
    #[fail(display = "can't use role from another team")]
    BadRole,
}

#[derive(Serialize)]
struct LogNewTeam<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct LogAddMember {
    user: i32,
    permissions: i32,
    role: Option<i32>,
}
