use actix_web::{
    HttpRequest,
    HttpResponse,
    http::StatusCode,
    web::{self, Json, Path, ServiceConfig},
};
use adaptarr_models::{Model, PermissionBits, Role, permissions::EditRole};
use adaptarr_web::{Created, Database, Session};
use diesel::Connection as _;
use serde::Deserialize;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .service(web::resource("/roles")
            .route(web::get().to(list_roles))
            .route(web::post().to(create_role))
        )
        .service(web::resource("/roles/{id}")
            .route(web::get().to(get_role))
            .route(web::put().to(update_role))
            .route(web::delete().to(delete_role))
        )
    ;
}

/// Get list of all roles.
///
/// ## Method
///
/// ```text
/// GET /roles
/// ```
fn list_roles(db: Database, session: Session)
-> Result<Json<Vec<<Role as Model>::Public>>> {
    let show_permissions = session.user(&db)?
        .permissions(true)
        .contains(PermissionBits::EDIT_ROLE);

    Ok(Json(Role::all(&db)?.get_public_full(&db, show_permissions)?))
}

#[derive(Deserialize)]
struct NewRole {
    name: String,
    #[serde(default)]
    permissions: PermissionBits,
}

/// Create a new role.
///
/// ## Method
///
/// ```text
/// POST /roles
/// ```
fn create_role(
    req: HttpRequest,
    db: Database,
    _: Session<EditRole>,
    data: Json<NewRole>,
) -> Result<Created<String, Json<<Role as Model>::Public>>> {
    let role = Role::create(&db, &data.name, data.permissions)?;
    let location = format!("{}/api/v1/roles/{}", req.app_config().host(), role.id);

    Ok(Created(location, Json(role.get_public_full(&db, true)?)))
}

/// Get a role by ID.
///
/// ## Method
///
/// ```text
/// GET /roles/:id
/// ```
fn get_role(db: Database, session: Session, id: Path<i32>)
-> Result<Json<<Role as Model>::Public>> {
    let show_permissions = session.user(&db)?
        .permissions(true)
        .contains(PermissionBits::EDIT_ROLE);

    Ok(Json(Role::by_id(&db, *id)?.get_public_full(&db, show_permissions)?))
}

#[derive(Deserialize)]
struct RoleUpdate {
    name: Option<String>,
    permissions: Option<PermissionBits>,
}

/// Update a role.
///
/// ## Method
///
/// ```text
/// PUT /roles/:id
/// ```
fn update_role(
    db: Database,
    _: Session<EditRole>,
    id: Path<i32>,
    update: Json<RoleUpdate>,
) -> Result<Json<<Role as Model>::Public>> {
    let mut role = Role::by_id(&db, id.into_inner())?;

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

    Ok(Json(role.get_public_full(&db, true)?))
}

/// Delete a role.
///
/// ## Method
///
/// ```text
/// DELETE /roles/:id
/// ```
fn delete_role(db: Database, _: Session<EditRole>, id: Path<i32>)
-> Result<HttpResponse> {
    Role::by_id(&db, id.into_inner())?.delete(&db)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}
