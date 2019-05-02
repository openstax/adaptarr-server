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
    models::{
        File,
        User,
        draft::PublicData as DraftData,
        editing::Process,
        module::{Module, PublicData as ModuleData},
        xref_target::PublicData as XrefTargetData,
    },
    import::{ImportModule, ReplaceModule},
    multipart::Multipart,
    permissions::{EditModule, ManageProcess},
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    session::Session,
    util::FormOrJson,
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
        .resource("/modules/{id}", |r| {
            r.get().api_with(get_module);
            r.post().api_with(begin_process);
            r.put().api_with_async(replace_module);
            r.delete().f(delete_module);
        })
        .resource("/modules/{id}/comments", |r| {
            r.get().f(list_comments);
            r.post().f(add_comment);
        })
        .api_route("/modules/{id}/files", Method::GET, list_files)
        .api_route("/modules/{id}/files/{name}", Method::GET, get_file)
        .api_route("/modules/{id}/xref-targets", Method::GET, list_xref_targets)
        .api_route("/modules/{id}/books", Method::GET, list_containing_books)
}

type Result<T, E=Error> = std::result::Result<T, E>;

#[derive(Debug, Deserialize)]
pub struct NewModule {
    title: String,
    language: String,
}

/// Get list of all modules.
///
/// ## Method
///
/// ```text
/// Get /modules
/// ```
pub fn list_modules(
    state: actix_web::State<State>,
    _session: Session,
) -> Result<Json<Vec<ModuleData>>> {
    let db = state.db.get()?;
    let modules = Module::all(&*db)?;
    Ok(Json(modules.into_iter()
        .map(|module| module.get_public())
        .collect()))
}

/// Create a new empty module.
///
/// ## Method
///
/// ```text
/// POST /modules
/// Content-Type: application/json
/// ```
pub fn create_module(
    state: actix_web::State<State>,
    _session: Session<EditModule>,
    data: Json<NewModule>,
) -> Result<Json<ModuleData>> {
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

    let module = Module::create::<&str, _>(
        &*db, &data.title, &data.language, index, std::iter::empty())?;

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
/// ```text
/// POST /modules
/// Content-Type: multipart/form-data
/// ```
pub fn create_module_from_zip(
    state: actix_web::State<State>,
    _session: Session<EditModule>,
    data: Multipart<NewModuleZip>,
) -> impl Future<Item = Json<ModuleData>, Error = Error> {
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
/// ```text
/// GET /modules/:id
/// ```
pub fn get_module(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<Uuid>,
) -> Result<Json<ModuleData>> {
    let db = state.db.get()?;
    let module = Module::by_id(&*db, id.into_inner())?;

    Ok(Json(module.get_public()))
}

#[derive(Deserialize)]
pub struct BeginProcess {
    process: i32,
    /// Mapping from slot IDs to user IDs.
    slots: Vec<(i32, i32)>,
}

/// Begin a new editing process for a module.
///
/// ## Method
///
/// ```text
/// POST /modules/:id
/// ```
pub fn begin_process(
    state: actix_web::State<State>,
    session: Session<ManageProcess>,
    id: Path<Uuid>,
    data: FormOrJson<BeginProcess>,
) -> Result<Json<DraftData>> {
    let db = state.db.get()?;
    let data = data.into_inner();
    let module = Module::by_id(&*db, id.into_inner())?;
    let process = Process::by_id(&*db, data.process)?;
    let version = process.get_current(&*db)?;

    let slots = data.slots.into_iter()
        .map(|(slot, user)| Ok((
            version.get_slot(&*db, slot)?,
            User::by_id(&*db, user)?,
        )))
        .collect::<Result<Vec<_>, Error>>()?;

    let draft = module.begin_process(&*db, &version, slots)?;

    draft.get_public(&*db, session.user_id()).map(Json).map_err(Into::into)
}

#[derive(Debug, Deserialize)]
pub struct ModuleUpdate {
}

/// Replace module with contents of a ZIP archive.
///
/// ## Method
///
/// ```text
/// PUT /modules/:id
/// ```
pub fn replace_module(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    _session: Session<EditModule>,
    id: Path<Uuid>,
) -> impl Future<Item = Json<ModuleData>, Error = Error> {
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
/// ```text
/// DELTE /modules/:id
/// ```
pub fn delete_module(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Get comments on a module.
///
/// ## Method
///
/// ```text
/// GET /modules/:id/comments
/// ```
pub fn list_comments(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Add a comment to a module
///
/// ## Method
///
/// ```text
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
/// ```text
/// GET /modules/:id/files
/// ```
pub fn list_files(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<Uuid>,
) -> Result<Json<Vec<FileInfo>>> {
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
/// ```text
/// GET /modules/:id/files/:name
/// ```
pub fn get_file(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(Uuid, String)>,
) -> Result<impl Responder> {
    let (id, name) = path.into_inner();
    let db = state.db.get()?;
    let module = Module::by_id(&*db, id)?;

    Ok(module.get_file(&*db, &name)?
        .stream(&state.config))
}

/// Get a list of all possible cross-reference targets within a module.
///
/// ## Method
///
/// ```text
/// GET /modules/:id/xref-targets
/// ```
pub fn list_xref_targets(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<Uuid>,
) -> Result<Json<Vec<XrefTargetData>>> {
    let db = state.db.get()?;
    let module = Module::by_id(&*db, *id)?;

    let targets = module.xref_targets(&*db)?
        .into_iter()
        .map(|x| x.get_public())
        .collect();

    Ok(Json(targets))
}

/// Get a list of all books containing this module in them.
///
/// ## Method
///
/// ```text
/// GET /modules/:id/books
/// ```
pub fn list_containing_books(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<Uuid>,
) -> Result<Json<Vec<Uuid>>> {
    let db = state.db.get()?;
    let module = Module::by_id(&*db, *id)?;

    module.get_books(&*db)
        .map(Json)
        .map_err(Into::into)
}

fn de_optional_null<'de, T, D>(de: D) -> std::result::Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(de).map(Some)
}
