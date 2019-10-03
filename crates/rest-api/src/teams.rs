use actix_web::{
    HttpResponse,
    HttpRequest,
    web::{self, Json, Path, ServiceConfig},
    http::StatusCode,
};
use adaptarr_error::Error;
use adaptarr_models::{
    Invite,
    Model,
    Role,
    Team,
    TeamMember,
    TeamPermissions,
    User,
    permissions::{
        AddMember,
        EditRole,
        ManageTeams,
        PermissionBits,
        RemoveMember,
        SystemPermissions,
    },
};
use adaptarr_web::{Created, Database, FormOrJson, Session, TeamScoped};
use diesel::Connection as _;
use serde::Deserialize;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .service(web::resource("/teams")
            .route(web::get().to(list_teams))
            .route(web::post().to(create_team))
        )
        .service(web::resource("/teams/{id}")
            .name("team")
            .route(web::get().to(get_team))
            .route(web::put().to(update_team))
        )
        .service(web::resource("/teams/{id}/roles")
            .route(web::get().to(list_roles))
            .route(web::post().to(create_role))
        )
        .service(web::resource("/teams/{id}/roles/{role}")
            .name("role")
            .route(web::get().to(get_role))
            .route(web::put().to(update_role))
            .route(web::delete().to(delete_role))
        )
        .service(web::resource("/teams/{id}/members")
            .route(web::get().to(list_members))
            .route(web::post().to(add_member))
        )
        .service(web::resource("/teams/{id}/members/{member}")
            .name("member")
            .route(web::get().to(get_member))
            .route(web::put().to(update_member))
            .route(web::delete().to(delete_member))
        )
    ;
}

/// Get list of all teams.
///
/// ## Method
///
/// ```
/// GET /teams
/// ```
fn list_teams(db: Database, session: Session)
-> Result<Json<Vec<<Team as Model>::Public>>> {
    let teams = if session.permissions().contains(SystemPermissions::MANAGE_TEAM) {
        Team::all(&db)?
    } else {
        session.user(&db)?.get_teams(&db)?
    };

    Ok(Json(teams.get_public_full(&db, &())?))
}

#[derive(Deserialize)]
struct NewTeam {
    name: String,
}

/// Create a new team.
///
/// ## Method
///
/// ```
/// POST /teams
/// ```
fn create_team(
    req: HttpRequest,
    db: Database,
    _: Session<ManageTeams>,
    data: FormOrJson<NewTeam>,
) -> Result<Created<String, Json<<Team as Model>::Public>>> {
    let team = Team::create(&db, &data.name)?;
    let location = req.url_for("team", &[team.id().to_string()])?.to_string();

    Ok(Created(location, Json(team.get_public_full(&db, &())?)))
}

/// Get a team by ID.
///
/// ## Method
///
/// ```text
/// GET /teams/:id
/// ```
fn get_team(db: Database, _: TeamScoped<Team>, id: Path<i32>)
-> Result<Json<<Team as Model>::Public>> {
    Ok(Json(Team::by_id(&db, *id)?.get_public_full(&db, &())?))
}

#[derive(Deserialize)]
struct TeamUpdate {
    name: String,
}

/// Modify a team.
///
/// ## Method
///
/// ```text
/// PUT /teams/:id
/// ```
fn update_team(
    db: Database,
    _: Session<ManageTeams>,
    _: TeamScoped<Team>,
    id: Path<i32>,
    update: FormOrJson<TeamUpdate>,
) -> Result<Json<<Team as Model>::Public>> {
    let mut team = Team::by_id(&db, *id)?;

    team.set_name(&db, &update.name)?;

    Ok(Json(team.get_public_full(&db, &())?))
}

/// Get list of all roles in a team.
///
/// ## Method
///
/// ```text
/// GET /teams/:id/roles
/// ```
fn list_roles(db: Database, scope: TeamScoped<Team>, id: Path<i32>)
-> Result<Json<Vec<<Role as Model>::Public>>> {
    let show_permissions =
        scope.permissions().contains(TeamPermissions::EDIT_ROLE);

    Ok(Json(Team::by_id(&db, *id)?
        .get_roles(&db)?
        .get_public_full(&db, &show_permissions)?))
}

#[derive(Deserialize)]
struct NewRole {
    name: String,
    #[serde(default = "TeamPermissions::empty")]
    permissions: TeamPermissions,
}

/// Create a new role.
///
/// ## Method
///
/// ```text
/// POST /teams/:id/roles
/// ```
fn create_role(
    req: HttpRequest,
    db: Database,
    scope: TeamScoped<Team, EditRole>,
    data: Json<NewRole>,
) -> Result<Created<String, Json<<Role as Model>::Public>>> {
    let team = scope.resource();
    let role = Role::create(&db, team, &data.name, data.permissions)?;
    let location = req.url_for(
        "role", &[team.id().to_string(), role.id().to_string()])?.to_string();

    Ok(Created(location, Json(role.get_public_full(&db, &true)?)))
}

/// Get a role by ID.
///
/// ## Method
///
/// ```text
/// GET /teams/:id/roles/:role
/// ```
fn get_role(db: Database, member: TeamScoped<Team>, path: Path<(i32, i32)>)
-> Result<Json<<Role as Model>::Public>> {
    let (team_id, role_id) = path.into_inner();
    let show_permissions =
        member.permissions().contains(TeamPermissions::EDIT_ROLE);

    Ok(Json(Team::by_id(&db, team_id)?
        .get_role(&db, role_id)?
        .get_public_full(&db, &show_permissions)?))
}

#[derive(Deserialize)]
struct RoleUpdate {
    name: Option<String>,
    permissions: Option<TeamPermissions>,
}

/// Update a role.
///
/// ## Method
///
/// ```text
/// PUT /teams/:id/roles/:role
/// ```
fn update_role(
    db: Database,
    _: TeamScoped<Team, EditRole>,
    path: Path<(i32, i32)>,
    update: Json<RoleUpdate>,
) -> Result<Json<<Role as Model>::Public>> {
    let (team_id, role_id) = path.into_inner();
    let mut role = Team::by_id(&db, team_id)?.get_role(&db, role_id)?;

    let db = &db;
    db.transaction::<_, diesel::result::Error, _>(|| {
        if let Some(ref name) = update.name {
            role.set_name(db, name)?;
        }

        if let Some(permissions) = update.permissions {
            role.set_permissions(db, permissions)?;
        }

        Ok(())
    })?;

    Ok(Json(role.get_public_full(&db, &true)?))
}

/// Delete a role.
///
/// ## Method
///
/// ```text
/// DELETE /teams/:id/roles/:role
/// ```
fn delete_role(
    db: Database,
    _: TeamScoped<Team, EditRole>,
    path: Path<(i32, i32)>,
) -> Result<HttpResponse> {
    let (team_id, role_id) = path.into_inner();

    Team::by_id(&db, team_id)?.get_role(&db, role_id)?.delete(&db)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

/// List all members in a team.
///
/// ## Method
///
/// ```text
/// GET /teams/:id/members
/// ```
fn list_members(db: Database, scope: TeamScoped<Team>)
-> Result<Json<Vec<<TeamMember as Model>::Public>>> {
    let show_permissions =
        scope.permissions().contains(TeamPermissions::EDIT_ROLE);

    Ok(Json(scope.resource()
        .get_members(&db)?
        .get_public_full(&db, &show_permissions)?))
}

#[derive(Deserialize)]
struct NewMember {
    user: UserRef,
    permissions: TeamPermissions,
    role: Option<i32>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum UserRef {
    ById(i32),
    ByEmail(String),
}

/// Add a member to a team.
///
/// ## Method
///
/// ```text
/// POST /teams/:id/members
/// ```
fn add_member(
    db: Database,
    scope: TeamScoped<Team, AddMember>,
    new: FormOrJson<NewMember>,
) -> Result<HttpResponse> {
    let NewMember { user, permissions, role } = new.into_inner();

    let user = match user {
        UserRef::ById(id) => User::by_id(&db, id)?,
        UserRef::ByEmail(email) => User::by_email(&db, &email)?,
    };

    let permissions = permissions & scope.permissions();
    let team = scope.into_resource();
    let role = role.map(|id| team.get_role(&db, id)).transpose()?;
    let locale = user.locale();
    let invite = Invite::create_for_existing(
        &db, team, role.as_ref(), permissions, user)?;

    invite.do_send_mail(locale);

    Ok(HttpResponse::new(StatusCode::ACCEPTED))
}

/// Get a specific member of a team.
///
/// ## Method
///
/// ```text
/// GET /teams/:id/members/:member
/// ```
fn get_member(db: Database, scope: TeamScoped<Team>, path: Path<(i32, i32)>)
-> Result<Json<<TeamMember as Model>::Public>> {
    let (_, member_id) = path.into_inner();
    let user = User::by_id(&db, member_id)?;
    let member = scope.resource().get_member(&db, &user)?;
    let show_permissions =
        scope.permissions().contains(TeamPermissions::EDIT_ROLE);

    Ok(Json(member.get_public_full(&db, &show_permissions)?))
}

#[derive(Deserialize)]
struct MemberUpdate {
    permissions: Option<TeamPermissions>,
    #[serde(default, deserialize_with = "adaptarr_util::de_optional_null")]
    role: Option<Option<i32>>,
}

/// Update a member of a team.
///
/// ## Method
///
/// ```text
/// PUT /teams/:id/members/:member
/// ```
fn update_member(
    db: Database,
    scope: TeamScoped<Team>,
    path: Path<(i32, i32)>,
    update: FormOrJson<MemberUpdate>,
) -> Result<Json<<TeamMember as Model>::Public>> {
    let (_, member_id) = path.into_inner();
    let user = User::by_id(&db, member_id)?;
    let team = scope.resource();
    let mut member = team.get_member(&db, &user)?;

    db.transaction::<_, Error, _>(|| {
        if let Some(permissions) = update.permissions {
            scope.permissions().require(
                TeamPermissions::EDIT_MEMBER_PERMISSIONS)?;

            member.set_permissions(
                &db,
                member.permissions() & !scope.permissions() | permissions,
            )?;
        }

        if let Some(role) = update.role {
            scope.permissions().require(TeamPermissions::ASSIGN_ROLE)?;

            let role = role.map(|id| team.get_role(&db, id)).transpose()?;
            member.set_role(&db, role)?;
        }

        Ok(())
    })?;

    let show_permissions =
        scope.permissions().contains(TeamPermissions::EDIT_ROLE);

    Ok(Json(member.get_public_full(&db, &show_permissions)?))
}

/// Remove a user from a team.
///
/// ## Method
///
/// ```text
/// DELETE /teams/:id/members/:member
/// ```
fn delete_member(
    db: Database,
    scope: TeamScoped<Team, RemoveMember>,
    path: Path<(i32, i32)>,
) -> Result<HttpResponse> {
    let (_, member_id) = path.into_inner();
    let user = User::by_id(&db, member_id)?;

    scope.resource().get_member(&db, &user)?.delete(&db)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}
