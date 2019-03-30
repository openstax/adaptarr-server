use actix_web::{App, HttpResponse, Json, Path};
use diesel::{Connection as _};

use crate::{
    models::role::{Role, PublicData as RoleData},
    permissions::{EditRole, PermissionBits},
};
use super::{Error, RouteExt, State, session::Session};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/roles",|r| {
            r.get().api_with(list_roles);
            r.post().api_with(create_role);
        })
        .resource("/roles/{id}", |r| {
            r.get().api_with(get_role);
            r.put().api_with(update_role);
            r.delete().api_with(delete_role);
        })
}

type Result<T, E=Error> = std::result::Result<T, E>;

/// Get list of all roles.
///
/// ## Method
///
/// ```
/// GET /roles
/// ```
pub fn list_roles(
    state: actix_web::State<State>,
    session: Session,
) -> Result<Json<Vec<RoleData>>> {
    let db = state.db.get()?;
    let show_permissions = session.user(&*db)?
        .permissions(true)
        .contains(PermissionBits::EDIT_ROLE);

    Role::all(&*db)
        .map(|v| v.into_iter().map(|r| r.get_public(show_permissions)).collect())
        .map(Json)
        .map_err(Into::into)
}

#[derive(Deserialize)]
pub struct NewRole {
    name: String,
    #[serde(default)]
    permissions: PermissionBits,
}

/// Create a new role.
///
/// ## Method
///
/// ```
/// POST /roles
/// ```
pub fn create_role(
    state: actix_web::State<State>,
    _session: Session<EditRole>,
    data: Json<NewRole>,
) -> Result<Json<RoleData>> {
    let db = state.db.get()?;
    let role = Role::create(&*db, &data.name, data.permissions)?;

    Ok(Json(role.get_public(true)))
}

/// Get a role by ID.
///
/// ## Method
///
/// ```
/// GET /roles/:id
/// ```
pub fn get_role(
    state: actix_web::State<State>,
    session: Session,
    id: Path<i32>,
) -> Result<Json<RoleData>> {
    let db = state.db.get()?;
    let role = Role::by_id(&*db, id.into_inner())?;
    let show_permissions = session.user(&*db)?
        .permissions(true)
        .contains(PermissionBits::EDIT_ROLE);

    Ok(Json(role.get_public(show_permissions)))
}

#[derive(Deserialize)]
pub struct RoleUpdate {
    name: Option<String>,
    permissions: Option<PermissionBits>,
}

/// Update a role.
///
/// ## Method
///
/// ```
/// PUT /roles/:id
/// ```
pub fn update_role(
    state: actix_web::State<State>,
    _session: Session<EditRole>,
    id: Path<i32>,
    update: Json<RoleUpdate>,
) -> Result<Json<RoleData>> {
    let db = state.db.get()?;
    let mut role = Role::by_id(&*db, id.into_inner())?;

    let dbcon = &*db;
    dbcon.transaction::<_, Error, _>(|| {
        if let Some(ref name) = update.name {
            role.set_name(dbcon, name)?;
        }

        if let Some(permissions) = update.permissions {
            role.set_permissions(dbcon, permissions)?;
        }

        Ok(())
    })?;

    Ok(Json(role.get_public(true)))
}

/// Delete a role.
///
/// ## Method
///
/// ```
/// DELETE /roles/:id
/// ```
pub fn delete_role(
    state: actix_web::State<State>,
    _session: Session<EditRole>,
    id: Path<i32>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;

    Role::by_id(&*db, id.into_inner())?.delete(&*db)?;

    Ok(HttpResponse::Ok().finish())
}
