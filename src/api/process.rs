use actix_web::{App, HttpResponse, Json, Path};

use crate::{
    models::editing::{Process, ProcessData, Version, VersionData, structure},
    permissions::EditProcess,
};
use super::{Error, RouteExt, State, session::Session};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/processes", |r| {
            r.get().api_with(list_processes);
            r.post().api_with(create_process);
        })
        .scope("/processes", |scope| scope
            .resource("/{id}", |r| {
                r.get().api_with(get_process);
                r.put().api_with(update_process);
                r.delete().api_with(delete_process);
            })
            .resource("/{id}/structure", |r| {
                r.get().api_with(get_process_structure);
                r.put().api_with(create_version);
            })
            .resource("/{id}/versions", |r| {
                r.get().api_with(list_process_versions);
            })
            .resource("/{id}/versions/{version}", |r| {
                r.get().api_with(get_process_version);
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
) -> Result<Json<ProcessData>> {
    unimplemented!()
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
) -> Result<Json<ProcessData>> {
    unimplemented!()
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

    Ok(HttpResponse::NoContent().finish())
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
/// PUT /processes/:id/structure
/// ```
pub fn create_version(
    state: actix_web::State<State>,
    _session: Session<EditProcess>,
    id: Path<i32>,
) -> Result<Json<VersionData>> {
    unimplemented!()
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
