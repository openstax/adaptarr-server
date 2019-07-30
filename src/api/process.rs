use actix_web::{App, HttpResponse, Json, Path, http::{StatusCode, Method}};
use diesel::Connection as _;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    db::Connection,
    models::{
        Role,
        draft::{Draft, PublicData as DraftData},
        editing::{
            Process,
            ProcessData,
            Version,
            VersionData,
            link::{Link, PublicData as LinkData},
            slot::{Slot, PublicData as SlotData},
            step::{Step, PublicData as StepData},
            structure,
        },
    },
    permissions::EditProcess,
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    session::Session,
    util::{Created, FormOrJson},
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/processes", |r| {
            r.get().api_with(list_processes);
            r.post().api_with(create_process);
        })
        .scope("/processes", |scope| scope
            .api_route("/slots", Method::POST, assign_to_slot)
            .api_route("/slots/free",  Method::GET, list_free_slots)
            .resource("/{id}", |r| {
                r.get().api_with(get_process);
                r.put().api_with(update_process);
                r.delete().api_with(delete_process);
            })
            .api_route("/{id}/slots", Method::GET, list_slots_in_process)
            .resource("/{id}/slots/{slot}", |r| {
                r.get().api_with(get_slot_in_process);
                r.put().api_with(modify_slot_in_process);
            })
            .api_route("/{id}/steps", Method::GET, list_steps_in_process)
            .resource("/{id}/steps/{step}", |r| {
                r.get().api_with(get_step_in_process);
                r.put().api_with(modify_step_in_process);
            })
            .api_route("/{id}/steps/{step}/links",
                Method::GET, list_links_in_process)
            .resource("/{id}/steps/{step}/links/{slot}/{target}", |r| {
                r.get().api_with(get_link_in_process);
                r.put().api_with(modify_link_in_process);
            })
            .resource("/{id}/structure", |r| {
                r.get().api_with(get_process_structure);
            })
            .resource("/{id}/versions", |r| {
                r.get().api_with(list_process_versions);
                r.post().api_with(create_version);
            })
            .resource("/{id}/versions/{version}", |r| {
                r.get().api_with(get_process_version);
            })
            .api_route("/{id}/versions/{version}/slots",
                Method::GET, list_slots_in_version)
            .resource("/{id}/versions/{version}/slots/{slot}", |r| {
                r.get().api_with(get_slot_in_version);
                r.put().api_with(modify_slot_in_version)
            })
            .api_route("/{id}/versions/{version}/steps",
                Method::GET, list_steps_in_version)
            .resource("/{id}/versions/{version}/steps/{step}", |r| {
                r.get().api_with(get_step_in_version);
                r.put().api_with(modify_step_in_version);
            })
            .api_route("/{id}/versions/{version}/steps/{step}/links",
                Method::GET, list_links_in_version)
            .resource("/{id}/versions/{version}/steps/{step}/links/{slot}/{target}", |r| {
                r.get().api_with(get_link_in_version);
                r.put().api_with(modify_link_in_version);
            })
            .resource("/{id}/versions/{version}/structure", |r| {
                r.get().api_with(get_version_structure);
            })
        )
}

type Result<T, E=Error> = std::result::Result<T, E>;

/// Get list of all editing processes.
///
/// ## Method
///
/// ```text
/// GET /processes
/// ```
pub fn list_processes(
    state: actix_web::State<State>,
    _session: Session,
) -> Result<Json<Vec<ProcessData>>> {
    Process::all(&*state.db.get()?)
        .map(|v| v.iter().map(Process::get_public).collect())
        .map(Json)
        .map_err(Into::into)
}

/// Create a new editing process.
///
/// ## Method
///
/// ```text
/// POST /processes
/// ```
pub fn create_process(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    data: Json<structure::Process>,
) -> Result<Created<String, Json<ProcessData>>> {
    let db = state.db.get()?;
    let process = Process::create(&*db, &*data)?;
    let location = format!("{}/api/v1/processes/{}",
        state.config.server.domain, process.process().id);

    Ok(Created(location, Json(process.process().get_public())))
}

#[derive(Deserialize)]
pub struct AssignToSlot {
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
pub fn assign_to_slot(
    state: actix_web::State<State>,
    session: Session,
    data: FormOrJson<AssignToSlot>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let data = data.into_inner();
    let draft = Draft::by_id(&*db, data.draft)?;
    let user = session.user(&*db)?;

    Slot::by_id(&*db, data.slot)?
        .fill_with(&*db, &draft, &user)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

#[derive(Serialize)]
pub struct FreeSlot {
    #[serde(flatten)]
    slot: SlotData,
    draft: DraftData,
}

/// Get list of all unoccupied slots a user can take.
///
/// ## Method
///
/// ```text
/// GET /processes/slots/free
/// ```
pub fn list_free_slots(
    state: actix_web::State<State>,
    session: Session,
) -> Result<Json<Vec<FreeSlot>>> {
    let db = state.db.get()?;
    let user = session.user(&*db)?;
    let slots = Slot::all_free(&*db, user.role)?
        .into_iter()
        .map(|(draft, slot)| Ok(FreeSlot {
            slot: slot.get_public(&*db)?,
            draft: draft.get_public_small(),
        }))
        .collect::<Result<Vec<_>>>()?;

    Ok(Json(slots))
}

/// Get a process by ID.
///
/// ## Method
///
/// ```text
/// GET /processes/:id
/// ```
pub fn get_process(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<i32>,
) -> Result<Json<ProcessData>> {
    let db = state.db.get()?;
    let process = Process::by_id(&*db, id.into_inner())?;

    Ok(Json(process.get_public()))
}

#[derive(Deserialize)]
pub struct ProcessUpdate {
    name: String,
}

/// Update an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id
/// ```
pub fn update_process(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    id: Path<i32>,
    update: Json<ProcessUpdate>,
) -> Result<Json<ProcessData>> {
    let db = state.db.get()?;
    let mut process = Process::by_id(&*db, id.into_inner())?;

    process.set_name(&*db, &update.name)?;

    Ok(Json(process.get_public()))
}

/// Delete an editing process.
///
/// ## Method
///
/// ```text
/// DELETE /processes/:id
/// ```
pub fn delete_process(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    id: Path<i32>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;

    Process::by_id(&*db, id.into_inner())?
        .delete(&*db)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

/// Get list of all slots in an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/slots
/// ```
pub fn list_slots_in_process(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<i32>,
) -> Result<Json<Vec<SlotData>>> {
    let db = state.db.get()?;
    list_slots(&*db, &Process::by_id(&*db, *id)?.get_current(&*db)?)
}

/// Get details of a particular slot in an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/slots/:slot
/// ```
pub fn get_slot_in_process(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<(i32, i32)>,
) -> Result<Json<SlotData>> {
    let (process_id, slot_id) = id.into_inner();
    let db = state.db.get()?;

    get_slot(&*db, &Process::by_id(&*db, process_id)?.get_current(&*db)?, slot_id)
}

/// Modify a slot in an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/slots/:slot
/// ```
pub fn modify_slot_in_process(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    path: Path<(i32, i32)>,
    data: Json<SlotUpdate>,
) -> Result<Json<SlotData>> {
    let (process_id, slot_id) = path.into_inner();
    let db = state.db.get()?;

    modify_slot(
        &*db,
        &Process::by_id(&*db, process_id)?.get_current(&*db)?,
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
pub fn list_steps_in_process(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<i32>,
) -> Result<Json<Vec<StepData>>> {
    let db = state.db.get()?;

    list_steps(&*db, &Process::by_id(&*db, *id)?.get_current(&*db)?)
}

/// Get details of a particular step in an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/steps/:step
/// ```
pub fn get_step_in_process(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<(i32, i32)>,
) -> Result<Json<StepData>> {
    let (process_id, step_id) = id.into_inner();
    let db = state.db.get()?;

    get_step(&*db, &Process::by_id(&*db, process_id)?.get_current(&*db)?, step_id)
}

/// Modify a step in an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/steps/:step
/// ```
pub fn modify_step_in_process(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    path: Path<(i32, i32)>,
    data: Json<StepUpdate>,
) -> Result<Json<StepData>> {
    let (process_id, step_id) = path.into_inner();
    let db = state.db.get()?;
    let mut step = Process::by_id(&*db, process_id)?
        .get_current(&*db)?
        .get_step(&*db, step_id)?;

    modify_step(&*db, &mut step, data.into_inner())
}

/// Get list of all links in a particular step of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/steps/:step/links
/// ```
pub fn list_links_in_process(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32)>,
) -> Result<Json<Vec<LinkData>>> {
    let (process_id, step_id) = path.into_inner();
    let db = state.db.get()?;
    let step = Process::by_id(&*db, process_id)?
        .get_current(&*db)?
        .get_step(&*db, step_id)?;

    list_links(&*db, &step)
}

/// Get a particular link in a step of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/steps/:step/links/:slot/:target
/// ```
pub fn get_link_in_process(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32, i32, i32)>,
) -> Result<Json<LinkData>> {
    let (process_id, step_id, slot_id, target_id) = path.into_inner();
    let db = state.db.get()?;
    let step = Process::by_id(&*db, process_id)?
        .get_current(&*db)?
        .get_step(&*db, step_id)?;

    get_link(&*db, &step, slot_id, target_id)
}

/// Modify a particular link in a step of an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/steps/:step/links/:slot/:target
/// ```
pub fn modify_link_in_process(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    path: Path<(i32, i32, i32, i32)>,
    data: Json<LinkUpdate>,
) -> Result<Json<LinkData>> {
    let (process_id, step_id, slot_id, target_id) = path.into_inner();
    let db = state.db.get()?;
    let step = Process::by_id(&*db, process_id)?
        .get_current(&*db)?
        .get_step(&*db, step_id)?;

    modify_link(&*db, &step, slot_id, target_id, data.into_inner())
}

/// Get detailed process description.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/structure
/// ```
pub fn get_process_structure(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<i32>,
) -> Result<Json<structure::Process>> {
    let db = state.db.get()?;
    let process = Process::by_id(&*db, id.into_inner())?;
    let structure = process.get_current(&*db)?.get_structure(&*db)?;

    Ok(Json(structure))
}

/// Get list of all versions of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions
/// ```
pub fn list_process_versions(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<i32>,
) -> Result<Json<Vec<VersionData>>> {
    let db = state.db.get()?;
    let process = Process::by_id(&*db, id.into_inner())?;
    let versions = process.get_versions(&*db)?;

    Ok(Json(versions.iter().map(Version::get_public).collect()))
}

/// Create a new version of an editing process
///
/// ## Method
///
/// ```text
/// POST /processes/:id/versions
/// ```
pub fn create_version(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    id: Path<i32>,
    data: Json<structure::Process>,
) -> Result<Created<String, Json<VersionData>>> {
    let db = state.db.get()?;
    let process = Process::by_id(&*db, *id)?;
    let version = Version::create(&*db, process, &*data)?;
    let location = format!("{}/api/v1/processes/{}/versions/{}",
        state.config.server.domain, *id, version.id);

    Ok(Created(location, Json(version.get_public())))
}

/// Get a version by ID.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version
/// ```
pub fn get_process_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32)>,
) -> Result<Json<VersionData>> {
    let (process_id, version_id) = path.into_inner();
    let db = state.db.get()?;
    let version = Version::by_id(&*db, process_id, version_id)?;

    Ok(Json(version.get_public()))
}

/// Get list of all slots in a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/slots
/// ```
pub fn list_slots_in_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32)>,
) -> Result<Json<Vec<SlotData>>> {
    let (process_id, version_id) = path.into_inner();
    let db = state.db.get()?;
    list_slots(&*db, &Version::by_id(&*db, process_id, version_id)?)
}

fn list_slots(db: &Connection, version: &Version) -> Result<Json<Vec<SlotData>>> {
    version.get_slots(&*db)?
        .iter()
        .map(|s| s.get_public(&*db))
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
        .map_err(From::from)
}

/// Get details of a particular slot in a particular version of an editing
/// process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/slots/:slot
/// ```
pub fn get_slot_in_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32, i32)>,
) -> Result<Json<SlotData>> {
    let (process_id, version_id, slot_id) = path.into_inner();
    let db = state.db.get()?;

    get_slot(&*db, &Version::by_id(&*db, process_id, version_id)?, slot_id)
}

fn get_slot(db: &Connection, version: &Version, slot: i32) -> Result<Json<SlotData>> {
    version.get_slot(&*db, slot)?
        .get_public(&*db)
        .map(Json)
        .map_err(From::from)
}

/// Modify a slot in a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/versions/:version/slots/:slot
/// ```
pub fn modify_slot_in_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32, i32)>,
    data: Json<SlotUpdate>,
) -> Result<Json<SlotData>> {
    let (process_id, version_id, slot_id) = path.into_inner();
    let db = state.db.get()?;

    modify_slot(
        &*db,
        &Version::by_id(&*db, process_id, version_id)?,
        slot_id,
        data.into_inner(),
    )
}

#[derive(Deserialize)]
pub struct SlotUpdate {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    roles: Option<Vec<i32>>,
}

fn modify_slot(db: &Connection, version: &Version, slot: i32, data: SlotUpdate)
-> Result<Json<SlotData>> {
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

    slot.get_public(db).map(Json).map_err(From::from)
}

/// Get list of all steps in a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/steps
/// ```
pub fn list_steps_in_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32)>,
) -> Result<Json<Vec<StepData>>> {
    let (process_id, version_id) = path.into_inner();
    let db = state.db.get()?;

    list_steps(&*db, &Version::by_id(&*db, process_id, version_id)?)
}

fn list_steps(db: &Connection, version: &Version) -> Result<Json<Vec<StepData>>> {
    version.get_steps(&*db)?
        .iter()
        .map(|s| s.get_public(&*db, None))
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
        .map_err(From::from)
}

/// Get details of a particular step in a particular version of an editing
/// process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/steps/:step
/// ```
pub fn get_step_in_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32, i32)>,
) -> Result<Json<StepData>> {
    let (process_id, version_id, step_id) = path.into_inner();
    let db = state.db.get()?;

    get_step(&*db, &Version::by_id(&*db, process_id, version_id)?, step_id)
}

fn get_step(db: &Connection, version: &Version, step: i32)
-> Result<Json<StepData>> {
    version.get_step(&*db, step)?
        .get_public(&*db, None)
        .map(Json)
        .map_err(From::from)
}

/// Modify a step in a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/versions/:version/steps/:step
/// ```
pub fn modify_step_in_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32, i32)>,
    data: Json<StepUpdate>,
) -> Result<Json<StepData>> {
    let (process_id, version_id, step_id) = path.into_inner();
    let db = state.db.get()?;
    let mut step = Version::by_id(&*db, process_id, version_id)?
        .get_step(&*db, step_id)?;

    modify_step(&*db, &mut step, data.into_inner())
}

#[derive(Deserialize)]
pub struct StepUpdate {
    name: String,
}

fn modify_step(db: &Connection, step: &mut Step, data: StepUpdate)
-> Result<Json<StepData>> {
    step.set_name(db, &data.name)?;

    step.get_public(db, None).map(Json).map_err(From::from)
}

/// Get list of all links in a particular step in a version of an editing
/// process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/steps/:step/links
/// ```
pub fn list_links_in_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32, i32)>,
) -> Result<Json<Vec<LinkData>>> {
    let (process_id, version_id, step_id) = path.into_inner();
    let db = state.db.get()?;
    let step = Version::by_id(&*db, process_id, version_id)?
        .get_step(&*db, step_id)?;

    list_links(&*db, &step)
}

fn list_links(db: &Connection, step: &Step) -> Result<Json<Vec<LinkData>>> {
    Ok(Json(step.get_links(db, None)?
        .iter()
        .map(Link::get_public)
        .collect()))
}

/// Get details of a particular link in a step of a particular version of an
/// editing process.
///
/// ## Method
///
/// ```text
/// GET /processes/:id/versions/:version/steps/:step/links/:slot/:target
pub fn get_link_in_version(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32, i32, i32, i32)>
) -> Result<Json<LinkData>> {
    let (process_id, version_id, step_id, slot_id, target_id) = path.into_inner();
    let db = state.db.get()?;
    let step = Version::by_id(&*db, process_id, version_id)?
        .get_step(&*db, step_id)?;

    get_link(&*db, &step, slot_id, target_id)
}

fn get_link(db: &Connection, step: &Step, slot_id: i32, target_id: i32)
-> Result<Json<LinkData>> {
    Ok(Json(step.get_link(db, slot_id, target_id)?.get_public()))
}

/// Modify a link in a step of a particular version of an editing process.
///
/// ## Method
///
/// ```text
/// PUT /processes/:id/versions/:version/steps/:step/links/:slot/:target
/// ```
pub fn modify_link_in_version(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    path: Path<(i32, i32, i32, i32, i32)>,
    data: Json<LinkUpdate>,
) -> Result<Json<LinkData>> {
    let (process_id, version_id, step_id, slot_id, target_id) = path.into_inner();
    let db = state.db.get()?;
    let step = Version::by_id(&*db, process_id, version_id)?
        .get_step(&*db, step_id)?;

    modify_link(&*db, &step, slot_id, target_id, data.into_inner())
}

#[derive(Deserialize)]
pub struct LinkUpdate {
    name: String,
}

fn modify_link(
    db: &Connection,
    step: &Step,
    slot_id: i32,
    target_id: i32,
    data: LinkUpdate,
) -> Result<Json<LinkData>> {
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
pub fn get_version_structure(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(i32, i32)>,
) -> Result<Json<structure::Process>> {
    let (process_id, version_id) = path.into_inner();
    let db = state.db.get()?;
    let version = Version::by_id(&*db, process_id, version_id)?;
    let structure = version.get_structure(&*db)?;

    Ok(Json(structure))
}
