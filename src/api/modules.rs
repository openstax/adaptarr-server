use actix_web::{
    App,
    HttpMessage,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    Responder,
    http::Method,
    pred,
};
use futures::{Future, Stream, future};
use serde::de::{Deserialize, Deserializer};
use std::io::Write;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{
    events::{self, EventManagerAddrExt},
    models::{
        File,
        User,
        draft::PublicData as DraftData,
        module::{Module, PublicData as ModuleData},
    },
    import::{ImportModule, ReplaceModule},
    multipart::Multipart,
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    session::{Session, ElevatedSession},
    users::UserId,
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/modules", |r| {
            r.get().api_with(list_modules);
            r.post()
                .filter(pred::Header("Content-Type", "application/json"))
                .api_with(create_module);
            r.post()
                .api_with_async(create_module_from_zip);
        })
        .api_route("/modules/assigned/to/{user}", Method::GET, list_assigned)
        .resource("/modules/{id}", |r| {
            r.get().api_with(get_module);
            r.post().api_with(crete_draft);
            r.put()
                .filter(pred::Header("Content-Type", "application/json"))
                .api_with(update_module);
            r.put().api_with_async(replace_module);
            r.delete().f(delete_module);
        })
        .resource("/modules/{id}/comments", |r| {
            r.get().f(list_comments);
            r.post().f(add_comment);
        })
        .api_route("/modules/{id}/files", Method::GET, list_files)
        .api_route("/modules/{id}/files/{name}", Method::GET, get_file)
}

type Result<T, E=Error> = std::result::Result<T, E>;

#[derive(Debug, Deserialize)]
pub struct NewModule {
    title: String,
}

/// Get list of all modules.
///
/// ## Method
///
/// ```
/// Get /modules
/// ```
pub fn list_modules((
    state,
    _session,
): (
    actix_web::State<State>,
    Session,
)) -> Result<Json<Vec<ModuleData>>> {
    let db = state.db.get()?;
    let modules = Module::all(&*db)?;
    Ok(Json(modules.into_iter()
        .map(|module| module.get_public())
        .collect()))
}

/// Get list of all modules assigned to a particular user.
///
/// ## Method
///
/// ```
/// GET /modules/assigned/to/:user
/// ```
pub fn list_assigned((
    state,
    session,
    user,
): (
    actix_web::State<State>,
    Session,
    Path<UserId>,
)) -> Result<Json<Vec<ModuleData>>, Error> {
    let db = state.db.get()?;
    let user = user.get_user(&*state, &session)?;
    let modules = Module::assigned_to(&*db, user.id)?;
    Ok(Json(modules.into_iter()
        .map(|module| module.get_public())
        .collect()))
}

/// Create a new empty module.
///
/// ## Method
///
/// ```
/// POST /modules
/// Content-Type: application/json
/// ```
pub fn create_module((
    state,
    _session,
    data,
): (
    actix_web::State<State>,
    ElevatedSession,
    Json<NewModule>,
)) -> Result<Json<ModuleData>> {
    let db = state.db.get()?;

    let content = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
        <document xmlns="http://cnx.rice.edu/cnxml" cnxml-version="0.7" id="new" module-id="new">
            <title>{}</title>
            <content>
                <para/>
            </content>
        </document>
        "#,
        tera::escape_html(&data.title),
    );

    let index = File::from_data(&*db, &state.config, &content)?;

    let module = Module::create::<&str, _>(&*db, &data.title, index, std::iter::empty())?;

    Ok(Json(module.get_public()))
}

pub struct NewModuleZip {
    title: String,
    file: NamedTempFile,
}

from_multipart! {
    multipart NewModuleZip via _NewModuleZipImpl {
        title: String,
        file: NamedTempFile,
    }
}

/// Create a new module, populating it with contents of a ZIP archive.
///
/// ## Method
///
/// ```
/// POST /modules
/// Content-Type: multipart/form-data
/// ```
pub fn create_module_from_zip((
    state,
    _session,
    data,
): (
    actix_web::State<State>,
    ElevatedSession,
    Multipart<NewModuleZip>
)) -> impl Future<Item = Json<ModuleData>, Error = Error> {
    let NewModuleZip { title, file } = data.into_inner();
    state.importer.send(ImportModule { title, file })
        .from_err()
        .and_then(|r| future::result(r).from_err())
        .map(|module| Json(module.get_public()))
}

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
    let db = state.db.get()?;
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
pub fn crete_draft((
    state,
    session,
    id,
): (
    actix_web::State<State>,
    Session,
    Path<Uuid>,
)) -> Result<Json<DraftData>> {
    let db = state.db.get()?;
    let module = Module::by_id(&*db, id.into_inner())?;
    let draft = module.create_draft(&*db, session.user)?;

    Ok(Json(draft.get_public()))
}

#[derive(Debug, Deserialize)]
pub struct ModuleUpdate {
    #[serde(default, deserialize_with = "de_optional_null")]
    pub assignee: Option<Option<i32>>,
}

/// Update module
///
/// ## Method
///
/// ```
/// PUT /modules/:id
/// Content-Type: application/json
/// ```
pub fn update_module((
    state,
    session,
    id,
    update,
): (
    actix_web::State<State>,
    ElevatedSession,
    Path<Uuid>,
    Json<ModuleUpdate>,
)) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let module = Module::by_id(&*db, id.into_inner())?;

    use diesel::Connection;
    let dbconn = &*db;
    dbconn.transaction::<_, Error, _>(|| {
        if let Some(user) = update.assignee {
            module.set_assignee(dbconn, user)?;

            if let Some(id) = user {
                let user = User::by_id(dbconn, id)?;
                state.events.notify(user, events::Assigned {
                    who: session.user,
                    module: module.id(),
                });
            }
        }

        Ok(())
    })?;

    Ok(HttpResponse::Ok().finish())
}

/// Replace module with contents of a ZIP archive.
///
/// ## Method
///
/// ```
/// PUT /modules/:id
/// ```
pub fn replace_module((
    req,
    state,
    _session,
    id,
): (
    HttpRequest<State>,
    actix_web::State<State>,
    ElevatedSession,
    Path<Uuid>,
)) -> impl Future<Item = Json<ModuleData>, Error = Error> {
    future::result(
        state.db.get()
            .map_err(Into::into)
            .and_then(|db| Module::by_id(&*db, id.into_inner())
                .map_err(Into::into)))
        .and_then(|module| future::result(
            NamedTempFile::new()
                .map(|file| (module, file))
                .map_err(Into::into)))
        .and_then(move |(module, file)| {
            req.payload()
                .from_err()
                .fold(file, |mut file, chunk| {
                    match file.write_all(chunk.as_ref()) {
                        Ok(_) => future::ok(file),
                        Err(e) => future::err(e),
                    }
                })
                .map(|file| (module, file))
        })
        .and_then(move |(module, file)| {
            state.importer.send(ReplaceModule { module, file })
                .from_err()
        })
        .and_then(|r| future::result(r).from_err())
        .map(|module| Json(module.get_public()))
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

#[derive(Debug, Serialize)]
pub struct FileInfo {
    name: String,
    mime: String,
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
)) -> Result<Json<Vec<FileInfo>>> {
    let db = state.db.get()?;
    let module = Module::by_id(&*db, *id)?;

    let files = module.get_files(&*db)?
        .into_iter()
        .map(|(name, file)| FileInfo {
            name,
            mime: file.into_db().mime,
        })
        .collect();

    Ok(Json(files))
}

/// Get a file from a module.
///
/// ## Method
///
/// ```
/// GET /modules/:id/files/:name
/// ```
pub fn get_file((
    state,
    _session,
    path,
): (
    actix_web::State<State>,
    Session,
    Path<(Uuid, String)>,
)) -> Result<impl Responder> {
    let (id, name) = path.into_inner();
    let db = state.db.get()?;
    let module = Module::by_id(&*db, id)?;

    Ok(module.get_file(&*db, &name)?
        .stream(&state.config))
}

fn de_optional_null<'de, T, D>(de: D) -> std::result::Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(de).map(Some)
}
