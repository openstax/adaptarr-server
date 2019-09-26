use actix_web::{
    HttpRequest,
    HttpResponse,
    http::StatusCode,
    web::{self, Json, Path, ServiceConfig},
};
use adaptarr_error::Error;
use adaptarr_models::{
    Draft,
    Model,
    Role,
    Team,
    db::Connection,
    editing::{Link, Process, Slot, Step, Version, structure},
    permissions::{EditProcess, PermissionBits, TeamPermissions},
};
use adaptarr_web::{Created, Database, FormOrJson, Session, TeamScoped};
use diesel::Connection as _;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .service(web::resource("/processes")
            .route(web::get().to(list_processes))
            .route(web::post().to(create_process))
        )
        .service(web::scope("/processes")
            .route("/slots", web::post().to(assign_to_slot))
            .route("/slots/free", web::get().to(list_free_slots))
            .service(web::resource("/{id}")
                .route(web::get().to(get_process))
                .route(web::put().to(update_process))
                .route(web::delete().to(delete_process))
            )
            .route("/{id}/slots", web::get().to(list_slots_in_process))
            .service(web::resource("/{id}/slots/{slot}")
                .route(web::get().to(get_slot_in_process))
                .route(web::put().to(modify_slot_in_process))
            )
            .route("/{id}/steps", web::get().to(list_steps_in_process))
            .service(web::resource("/{id}/steps/{step}")
                .route(web::get().to(get_step_in_process))
                .route(web::put().to(modify_step_in_process))
            )
            .route("/{id}/steps/{step}/links",
                web::get().to(list_links_in_process))
            .service(web::resource("/{id}/steps/{step}/links/{slot}/{target}")
                .route(web::get().to(get_link_in_process))
                .route(web::put().to(modify_link_in_process))
            )
            .service(web::resource("/{id}/structure")
                .route(web::get().to(get_process_structure))
            )
            .service(web::resource("/{id}/versions")
                .route(web::get().to(list_process_versions))
                .route(web::post().to(create_version))
            )
            .service(web::resource("/{id}/versions/{version}")
                .route(web::get().to(get_process_version))
            )
            .route("/{id}/versions/{version}/slots",
                web::get().to(list_slots_in_version))
            .service(web::resource("/{id}/versions/{version}/slots/{slot}")
                .route(web::get().to(get_slot_in_version))
                .route(web::put().to(modify_slot_in_version))
            )
            .route("/{id}/versions/{version}/steps",
                web::get().to(list_steps_in_version))
            .service(web::resource("/{id}/versions/{version}/steps/{step}")
                .route(web::get().to(get_step_in_version))
                .route(web::put().to(modify_step_in_version))
            )
            .route("/{id}/versions/{version}/steps/{step}/links",
                web::get().to(list_links_in_version))
            .service(web::resource("/{id}/versions/{version}/steps/{step}/links/{slot}/{target}")
                .route(web::get().to(get_link_in_version))
                .route(web::put().to(modify_link_in_version))
            )
            .service(web::resource("/{id}/versions/{version}/structure")
                .route(web::get().to(get_version_structure))
            )
        )
    ;
}

/// Get list of all editing processes.
///
/// ## Method
///
/// ```text
/// GET /processes
/// ```
fn list_processes(db: Database, session: Session)
-> Result<Json<Vec<<Process as Model>::Public>>> {
    let user = session.user(&db)?;
    let teams = user.get_team_ids(&db)?;

    Ok(Json(Process::by_team(&db, &teams)?.get_public()))
}

#[derive(Deserialize)]
struct NewProcess {
    team: i32,
    #[serde(flatten)]
    structure: structure::Process,
}

/// Create a new editing process.
///
/// ## Method
///
/// ```text
/// POST /processes
/// ```
fn create_process(
    req: HttpRequest,
    db: Database,
    session: Session,
    data: Json<NewProcess>,
) -> Result<Created<String, Json<<Process as Model>::Public>>> {
    let team = Team::by_id(&db, data.team)?;

    if !session.is_elevated {
        team.get_member(&db, &session.user(&db)?)?
            .permissions()
            .require(TeamPermissions::EDIT_PROCESS)?;
    }

    let process = Process::create(&db, &team, &data.structure)?;
    let location = format!("{}/api/v1/processes/{}",
        req.app_config().host(), process.process().id);

    Ok(Created(location, Json(process.process().get_public())))
}

#[derive(Deserialize)]
struct AssignToSlot {
    draft: Uuid,
    slot: i32,
}

/// Self-assign to a free slot.
///
/// ## Method
///
/// ```text
/// POST /processes/slots
/// ```
fn assign_to_slot(
    db: Database,
    session: Session,
    data: FormOrJson<AssignToSlot>,
) -> Result<HttpResponse> {
    let data = data.into_inner();
    let draft = Draft::by_id(&db, data.draft)?;
    let user = session.user(&db)?;

    Slot::by_id(&db, data.slot)?.fill_with(&db, &draft, &user)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

#[derive(Serialize)]
struct FreeSlot {
    #[serde(flatten)]
    slot: <Slot as Model>::Public,
    draft: <Draft as Model>::Public,
}

/// Get list of all unoccupied slots a user can take.
///
/// ## Method
///
/// ```text
/// GET /processes/slots/free
/// ```
fn list_free_slots(db: Database, session: Session)
-> Result<Json<Vec<FreeSlot>>> {
    let user = session.user(&db)?;

    Ok(Json(Slot::all_free(&db, &user)?
        .into_iter()
        .map(|(draft, slot)| Ok(FreeSlot {
            slot: slot.get_public_full(&db, &())?,
            draft: draft.get_public_full(&db, &user.id())?,
        }))
        .collect::<Result<Vec<_>>>()?))
}

/// Get a process by ID.
///
/// ## Method
///
/// ```text
/// GET /processes/:id
/// ```
fn get_process(scope: TeamScoped<Process>)
-> Result<Json<<Process as Model>::Public>> {
    Ok(Json(scope.resource().get_public()))
}

#[derive(Deserialize)]
struct ProcessUpdate {
    name: String,
}

/// Update an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id
/// ```
fn update_process(
    db: Database,
    scope: TeamScoped<Process, EditProcess>,
    update: Json<ProcessUpdate>,
) -> Result<Json<<Process as Model>::Public>> {
    let mut process = scope.into_resource();

    process.set_name(&db, &update.name)?;

    Ok(Json(process.get_public()))
}

/// Delete an editing process.
///
/// ## Method
///
/// ```text
/// DELETE /processes/:id
/// ```
fn delete_process(db: Database, scope: TeamScoped<Process, EditProcess>)
-> Result<HttpResponse> {
    scope.into_resource().delete(&db)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

/// Get list of all slots in an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/slots
/// ```
fn list_slots_in_process(db: Database, scope: TeamScoped<Process>)
-> Result<Json<Vec<<Slot as Model>::Public>>> {
    Ok(Json(scope.resource()
        .get_current(&db)?
        .get_slots(&db)?
        .get_public_full(&db, &())?))
}

/// Get details of a particular slot in an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/slots/:slot
/// ```
fn get_slot_in_process(
    db: Database,
    scope: TeamScoped<Process>,
    path: Path<(i32, i32)>,
) -> Result<Json<<Slot as Model>::Public>> {
    let (_, slot_id) = path.into_inner();

    Ok(Json(scope.resource()
        .get_current(&db)?
        .get_slot(&db, slot_id)?
        .get_public_full(&db, &())?))
}

/// Modify a slot in an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/slots/:slot
/// ```
fn modify_slot_in_process(
    db: Database,
    scope: TeamScoped<Process, EditProcess>,
    path: Path<(i32, i32)>,
    data: Json<SlotUpdate>,
) -> Result<Json<<Slot as Model>::Public>> {
    let (_, slot_id) = path.into_inner();

    modify_slot(
        &db,
        &scope.resource().get_current(&db)?,
        slot_id,
        data.into_inner(),
    )
}

/// Get list of all steps in an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/steps
/// ```
fn list_steps_in_process(db: Database, scope: TeamScoped<Process>)
-> Result<Json<Vec<<Step as Model>::Public>>> {
    Ok(Json(scope.resource()
        .get_current(&db)?
        .get_steps(&db)?
        .get_public()))
}

/// Get details of a particular step in an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/steps/:step
/// ```
fn get_step_in_process(
    db: Database,
    scope: TeamScoped<Process>,
    path: Path<(i32, i32)>,
) -> Result<Json<<Step as Model>::Public>> {
    let (_, step_id) = path.into_inner();

    Ok(Json(scope.resource()
        .get_current(&db)?
        .get_step(&db, step_id)?
        .get_public()))
}

/// Modify a step in an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/steps/:step
/// ```
fn modify_step_in_process(
    db: Database,
    scope: TeamScoped<Process, EditProcess>,
    path: Path<(i32, i32)>,
    data: Json<StepUpdate>,
) -> Result<Json<<Step as Model>::Public>> {
    let (_, step_id) = path.into_inner();
    let mut step = scope.resource()
        .get_current(&db)?
        .get_step(&db, step_id)?;

    modify_step(&db, &mut step, data.into_inner())
}

/// Get list of all links in a particular step of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/steps/:step/links
/// ```
fn list_links_in_process(
    db: Database,
    scope: TeamScoped<Process>,
    path: Path<(i32, i32)>,
) -> Result<Json<Vec<<Link as Model>::Public>>> {
    let (_, step_id) = path.into_inner();

    Ok(Json(scope.resource()
        .get_current(&db)?
        .get_step(&db, step_id)?
        .get_links(&db, None)?
        .get_public()))
}

/// Get a particular link in a step of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/steps/:step/links/:slot/:target
/// ```
fn get_link_in_process(
    db: Database,
    scope: TeamScoped<Process>,
    path: Path<(i32, i32, i32, i32)>,
) -> Result<Json<<Link as Model>::Public>> {
    let (_, step_id, slot_id, target_id) = path.into_inner();

    Ok(Json(scope.resource()
        .get_current(&db)?
        .get_step(&db, step_id)?
        .get_link(&db, slot_id, target_id)?
        .get_public()))
}

/// Modify a particular link in a step of an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/steps/:step/links/:slot/:target
/// ```
fn modify_link_in_process(
    db: Database,
    scope: TeamScoped<Process, EditProcess>,
    path: Path<(i32, i32, i32, i32)>,
    data: Json<LinkUpdate>,
) -> Result<Json<<Link as Model>::Public>> {
    let (_, step_id, slot_id, target_id) = path.into_inner();
    let step = scope.resource()
        .get_current(&db)?
        .get_step(&db, step_id)?;

    modify_link(&db, &step, slot_id, target_id, data.into_inner())
}

/// Get detailed process description.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/structure
/// ```
fn get_process_structure(db: Database, scope: TeamScoped<Process>)
-> Result<Json<structure::Process>> {
    Ok(Json(scope.resource().get_current(&db)?.get_structure(&db)?))
}

/// Get list of all versions of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions
/// ```
fn list_process_versions(db: Database, scope: TeamScoped<Process>)
-> Result<Json<Vec<<Version as Model>::Public>>> {
    Ok(Json(scope.resource().get_versions(&db)?.get_public()))
}

/// Create a new version of an editing process
///
/// ## Method
///
/// ```text
/// POST /processes/:id/versions
/// ```
fn create_version(
    req: HttpRequest,
    db: Database,
    scope: TeamScoped<Process, EditProcess>,
    id: Path<i32>,
    data: Json<structure::Process>,
) -> Result<Created<String, Json<<Version as Model>::Public>>> {
    let process = scope.into_resource();
    let version = Version::create(&db, process, &*data)?;
    let location = format!("{}/api/v1/processes/{}/versions/{}",
        req.app_config().host(), *id, version.id);

    Ok(Created(location, Json(version.get_public())))
}

/// Get a version by ID.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version
/// ```
fn get_process_version(
    db: Database,
    scope: TeamScoped<Process>,
    id: Path<(i32, i32)>,
) -> Result<Json<<Version as Model>::Public>> {
    let (_, version_id) = id.into_inner();

    Ok(Json(scope.resource().get_version(&db, version_id)?.get_public()))
}

/// Get list of all slots in a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/slots
/// ```
fn list_slots_in_version(
    db: Database,
    scope: TeamScoped<Process>,
    id: Path<(i32, i32)>,
) -> Result<Json<Vec<<Slot as Model>::Public>>> {
    let (_, version_id) = id.into_inner();

    Ok(Json(scope.resource()
        .get_version(&db, version_id)?
        .get_slots(&db)?
        .get_public_full(&db, &())?))
}

/// Get details of a particular slot in a particular version of an editing
/// process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/slots/:slot
/// ```
fn get_slot_in_version(
    db: Database,
    scope: TeamScoped<Process>,
    path: Path<(i32, i32, i32)>,
) -> Result<Json<<Slot as Model>::Public>> {
    let (_, version_id, slot_id) = path.into_inner();

    Ok(Json(scope.resource().get_version(&db, version_id)?
        .get_slot(&db, slot_id)?
        .get_public_full(&db, &())?))
}

/// Modify a slot in a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/versions/:version/slots/:slot
/// ```
fn modify_slot_in_version(
    db: Database,
    scope: TeamScoped<Process, EditProcess>,
    path: Path<(i32, i32, i32)>,
    data: Json<SlotUpdate>,
) -> Result<Json<<Slot as Model>::Public>> {
    let (_, version_id, slot_id) = path.into_inner();

    modify_slot(
        &db,
        &scope.resource().get_version(&db, version_id)?,
        slot_id,
        data.into_inner(),
    )
}

#[derive(Deserialize)]
struct SlotUpdate {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    roles: Option<Vec<i32>>,
}

fn modify_slot(db: &Connection, version: &Version, slot: i32, data: SlotUpdate)
-> Result<Json<<Slot as Model>::Public>> {
    let mut slot = version.get_slot(db, slot)?;

    db.transaction::<(), Error, _>(|| {
        if let Some(ref name) = data.name {
            slot.set_name(db, &name)?;
        }

        if let Some(ref roles) = data.roles {
            let roles = Role::by_ids(db, &roles)?;
            slot.set_role_limit(db, &roles)?;
        }

        Ok(())
    })?;

    Ok(Json(slot.get_public_full(db, &())?))
}

/// Get list of all steps in a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/steps
/// ```
fn list_steps_in_version(
    db: Database,
    scope: TeamScoped<Process>,
    id: Path<(i32, i32)>,
) -> Result<Json<Vec<<Step as Model>::Public>>> {
    let (_, version_id) = id.into_inner();

    Ok(Json(scope.resource().get_version(&db, version_id)?
        .get_steps(&db)?
        .get_public_full(&db, &(None, None))?))
}

/// Get details of a particular step in a particular version of an editing
/// process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/steps/:step
/// ```
fn get_step_in_version(
    db: Database,
    scope: TeamScoped<Process>,
    path: Path<(i32, i32, i32)>,
) -> Result<Json<<Step as Model>::Public>> {
    let (_, version_id, step_id) = path.into_inner();

    Ok(Json(scope.resource().get_version(&db, version_id)?
        .get_step(&db, step_id)?
        .get_public_full(&db, &(None, None))?))
}

/// Modify a step in a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/versions/:version/steps/:step
/// ```
fn modify_step_in_version(
    db: Database,
    scope: TeamScoped<Process, EditProcess>,
    path: Path<(i32, i32, i32)>,
    data: Json<StepUpdate>,
) -> Result<Json<<Step as Model>::Public>> {
    let (_, version_id, step_id) = path.into_inner();
    let mut step = scope.resource().get_version(&db, version_id)?
        .get_step(&db, step_id)?;

    modify_step(&db, &mut step, data.into_inner())
}

#[derive(Deserialize)]
struct StepUpdate {
    name: String,
}

fn modify_step(db: &Connection, step: &mut Step, data: StepUpdate)
-> Result<Json<<Step as Model>::Public>> {
    step.set_name(db, &data.name)?;

    Ok(Json(step.get_public_full(&db, &(None, None))?))
}

/// Get list of all links in a particular step in a version of an editing
/// process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/steps/:step/links
/// ```
fn list_links_in_version(
    db: Database,
    scope: TeamScoped<Process>,
    path: Path<(i32, i32, i32)>,
) -> Result<Json<Vec<<Link as Model>::Public>>> {
    let (_, version_id, step_id) = path.into_inner();
    let step = scope.resource()
        .get_version(&db, version_id)?
        .get_step(&db, step_id)?;

    list_links(&db, &step)
}

fn list_links(db: &Connection, step: &Step)
-> Result<Json<Vec<<Link as Model>::Public>>> {
    Ok(Json(step.get_links(db, None)?.get_public()))
}

/// Get details of a particular link in a step of a particular version of an
/// editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/steps/:step/links/:slot/:target
fn get_link_in_version(
    db: Database,
    scope: TeamScoped<Process>,
    path: Path<(i32, i32, i32, i32, i32)>,
) -> Result<Json<<Link as Model>::Public>> {
    let (_, version_id, step_id, slot_id, target_id) = path.into_inner();
    let step = scope.resource()
        .get_version(&db, version_id)?
        .get_step(&db, step_id)?;

    get_link(&db, &step, slot_id, target_id)
}

fn get_link(db: &Connection, step: &Step, slot_id: i32, target_id: i32)
-> Result<Json<<Link as Model>::Public>> {
    Ok(Json(step.get_link(db, slot_id, target_id)?.get_public()))
}

/// Modify a link in a step of a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/versions/:version/steps/:step/links/:slot/:target
/// ```
fn modify_link_in_version(
    db: Database,
    scope: TeamScoped<Process, EditProcess>,
    path: Path<(i32, i32, i32, i32, i32)>,
    data: Json<LinkUpdate>,
) -> Result<Json<<Link as Model>::Public>> {
    let (_, version_id, step_id, slot_id, target_id) = path.into_inner();
    let step = scope.resource()
        .get_version(&db, version_id)?
        .get_step(&db, step_id)?;

    modify_link(&db, &step, slot_id, target_id, data.into_inner())
}

#[derive(Deserialize)]
struct LinkUpdate {
    name: String,
}

fn modify_link(
    db: &Connection,
    step: &Step,
    slot_id: i32,
    target_id: i32,
    data: LinkUpdate,
) -> Result<Json<<Link as Model>::Public>> {
    let mut link = step.get_link(db, slot_id, target_id)?;

    link.set_name(&*db, &data.name)?;

    Ok(Json(link.get_public()))
}


/// Get detailed version description.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/structure
/// ```
fn get_version_structure(
    db: Database,
    scope: TeamScoped<Process>,
    id: Path<(i32, i32)>,
) -> Result<Json<structure::Process>> {
    let (_, version_id) = id.into_inner();

    Ok(Json(scope.resource().get_version(&db, version_id)?.get_structure(&db)?))
}
