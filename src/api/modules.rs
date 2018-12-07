use actix_web::{
    App,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    error::ErrorInternalServerError,
    http::Method,
};
use uuid::Uuid;

use crate::models::module::{Module, PublicData as ModuleData};
use super::{
    State,
    session::Session,
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/modules/{id}", |r| {
            r.get().with(get_module);
            r.post().f(crete_draft);
            r.delete().f(delete_module);
        })
        .resource("/modules/{id}/comments", |r| {
            r.get().f(list_comments);
            r.post().f(add_comment);
        })
        .route("/modules/{id}/files", Method::GET, list_files)
        .route("/modules/{id}/files/{name}", Method::GET, get_file)
}

type Result<T> = std::result::Result<T, actix_web::error::Error>;

/// Get a module by ID.
///
/// ## Method
///
/// ```
/// GET /modules/:id
/// ```
pub fn get_module((
    state,
    _session,
    id,
): (
    actix_web::State<State>,
    Session,
    Path<Uuid>,
)) -> Result<Json<ModuleData>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let module = Module::by_id(&*db, id.into_inner())?;

    Ok(Json(module.get_public()))
}

/// Create a new draft of a module.
///
/// ## Method
///
/// ```
/// POST /modules/:id
/// ```
pub fn crete_draft(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Delete a module
///
/// ## Method
///
/// ```
/// DELTE /modules/:id
/// ```
pub fn delete_module(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Get comments on a module.
///
/// ## Method
///
/// ```
/// GET /modules/:id/comments
/// ```
pub fn list_comments(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Add a comment to a module
///
/// ## Method
///
/// ```
/// POST /modules/:id/comments
/// ```
pub fn add_comment(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// List files in a module.
///
/// ## Method
///
/// ```
/// GET /modules/:id/files
/// ```
pub fn list_files((
    state,
    _session,
    id,
): (
    actix_web::State<State>,
    Session,
    Path<Uuid>,
)) -> Result<Json<Vec<String>>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let module = Module::by_id(&*db, *id)?;

    module.get_files(&*db)
        .map_err(|e| ErrorInternalServerError(e.to_string()))
        .map(Json)
}

/// Get a file from a module.
///
/// ## Method
///
/// ```
/// GET /modules/:id/files/:name
/// ```
pub fn get_file(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}
