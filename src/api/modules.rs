use actix_web::{
    App,
    AsyncResponder,
    HttpMessage,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    Responder,
    ResponseError,
    error::{ErrorInternalServerError, PayloadError, MultipartError},
    http::Method,
    multipart::{Field, Multipart, MultipartItem},
    pred,
};
use bytes::{Bytes, BytesMut, buf::FromBuf};
use futures::{Async, Future, Stream, Poll, future};
use serde::de::{Deserialize, Deserializer};
use std::io::Write;
use tempfile::{Builder as TempBuilder, NamedTempFile};
use uuid::Uuid;

use crate::{
    events::{self, EventManagerAddrExt},
    models::{
        File,
        User,
        draft::PublicData as DraftData,
        module::{Module, PublicData as ModuleData, FindModuleError},
    },
    import::{ImportError, ImportModule, ReplaceModule},
};
use super::{
    State,
    session::{Session, ElevatedSession},
    users::UserId,
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/modules", |r| {
            r.get().with(list_modules);
            r.post()
                .filter(pred::Header("Content-Type", "application/json"))
                .with(create_module);
            r.post()
                .with_async(create_module_from_zip);
        })
        .route("/modules/assigned/to/{user}", Method::GET, list_assigned)
        .resource("/modules/{id}", |r| {
            r.get().with(get_module);
            r.post().with(crete_draft);
            r.put()
                .filter(pred::Header("Content-Type", "application/json"))
                .with(update_module);
            r.put().with_async(replace_module);
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
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let modules = Module::all(&*db)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
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
)) -> Result<Json<Vec<ModuleData>>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let user = user.get_user(&*state, &session)?;
    let modules = Module::assigned_to(&*db, user.id)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
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
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

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

    let index = File::from_data(&*db, &state.config, &content)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    let module = Module::create::<&str, _>(&*db, &data.title, index, std::iter::empty())
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(Json(module.get_public()))
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
    req,
    state,
    _session,
): (
    HttpRequest<State>,
    actix_web::State<State>,
    ElevatedSession,
)) -> Box<dyn Future<Item = Json<ModuleData>, Error = LoadZipError>> {
    LoadZip::new(&state.config.storage.path, req.multipart())
        .and_then(move |(title, file)| {
            state.importer.send(ImportModule { title, file })
                .from_err()
        })
        .and_then(|r| future::result(r).from_err())
        .map(|module| Json(module.get_public()))
        .responder()
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
pub fn crete_draft((
    state,
    session,
    id,
): (
    actix_web::State<State>,
    Session,
    Path<Uuid>,
)) -> Result<Json<DraftData>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
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
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let module = Module::by_id(&*db, id.into_inner())?;

    use diesel::Connection;
    let dbconn = &*db;
    dbconn.transaction::<_, failure::Error, _>(|| {
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
    }).map_err(|e| ErrorInternalServerError(e.to_string()))?;

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
    session,
    id,
): (
    HttpRequest<State>,
    actix_web::State<State>,
    ElevatedSession,
    Path<Uuid>,
)) -> Box<dyn Future<Item = Json<ModuleData>, Error = LoadZipError>> {
    let importer = state.importer.clone();

    future::result(
        state.db.get()
            .map_err(LoadZipError::from)
            .and_then(|db| Module::by_id(&*db, id.into_inner())
                .map_err(LoadZipError::from)))
        .and_then(move |module| future::result(
            NamedTempFile::new_in(&state.config.storage.path)
                .map(|file| (module, file))
                .map_err(LoadZipError::from)))
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
            importer.send(ReplaceModule { module, file })
                .from_err()
        })
        .and_then(|r| future::result(r).from_err())
        .map(|module| Json(module.get_public()))
        .responder()
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
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
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

/// Process a multipart form containing title and ZIP file for a new module.
struct LoadZip<S> {
    from: Box<dyn Stream<Item = (String, Field<S>), Error = LoadZipError>>,
    fut: Option<Box<dyn Future<Item = ZipField, Error = LoadZipError>>>,

    title: Option<String>,
    file: Option<NamedTempFile>,
}

enum ZipField {
    Title(String),
    File(NamedTempFile),
}

impl<S> LoadZip<S>
where
    S: Stream<Item = Bytes, Error = PayloadError> + 'static,
{
    fn new(path: &std::path::Path, from: Multipart<S>)
        -> impl Future<Item = (String, NamedTempFile), Error = LoadZipError>
    {
        future::result(TempBuilder::new().tempfile_in(path))
            .from_err()
            .and_then(|file| LoadZip {
                from: Box::new(from
                    .map(process_item)
                    .flatten()),
                fut: None,
                title: None,
                file: Some(file),
            })
    }

    fn finish(&mut self) -> Poll<<Self as Future>::Item, LoadZipError> {
        let title = self.title.take().ok_or(LoadZipError::Incomplete)?;
        let file = self.file.take().ok_or(LoadZipError::Incomplete)?;

        Ok(Async::Ready((title, file)))
    }
}

impl<S> Future for LoadZip<S>
where
    S: Stream<Item = Bytes, Error = PayloadError> + 'static,
{
    type Item = (String, NamedTempFile);
    type Error = LoadZipError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut fut) = self.fut {
            let field = match fut.poll()? {
                Async::NotReady => return Ok(Async::NotReady),
                Async::Ready(field) => field,
            };

            match field {
                ZipField::Title(title) => self.title = Some(title),
                ZipField::File(file) => self.file = Some(file),
            }
        }
        self.fut = None;

        let (name, field) = match self.from.poll()? {
            Async::NotReady => return Ok(Async::NotReady),
            Async::Ready(Some(v)) => v,
            Async::Ready(None) => return self.finish(),
        };

        let fut: Box<dyn Future<Item = _, Error = _>> = match name.as_str() {
            "title" => Box::new(load_string(field)
                .map(ZipField::Title)),
            "file" => {
                // This unwrap() is safe, because self.file can only ever
                // be None if were reading into it, in which case we won't even
                // reach this point (or self.file will be replaced before we do)
                let file = self.file.take().unwrap();

                Box::new(field
                    .from_err()
                    .fold(file, |mut file, chunk| {
                        match file.write_all(chunk.as_ref()) {
                            Ok(_) => future::ok(file),
                            Err(e) => future::err(e),
                        }
                    })
                    .map(ZipField::File))
            }
            _ => return Err(LoadZipError::UnknownField(name)),
        };

        self.fut = Some(fut);
        self.poll()
    }
}

fn process_item<S>(item: MultipartItem<S>)
    -> Box<dyn Stream<Item = (String, Field<S>), Error = LoadZipError>>
where
    S: Stream<Item = Bytes, Error = PayloadError> + 'static,
{
    match item {
        MultipartItem::Field(field) => {
            let cd = match field.content_disposition() {
                Some(cd) => cd,
                None => return Box::new(future::err(
                    LoadZipError::ContentDispositionMissing
                ).into_stream()),
            };

            if !cd.is_form_data() {
                return Box::new(future::err(
                    LoadZipError::FieldNotFormData
                ).into_stream());
            }

            Box::new(future::result({
                cd.get_name()
                    .map(|name| (name.to_string(), field))
                    .ok_or(LoadZipError::UnnamedField)
            }).into_stream())
        },
        MultipartItem::Nested(mp) => Box::new(mp
            .map(process_item)
            .flatten()),
    }
}

fn load_string<S>(from: S) -> impl Future<Item = String, Error = LoadZipError>
where
    S: Stream<Item = Bytes, Error = MultipartError>,
{
    from
        .from_err()
        .fold(BytesMut::with_capacity(1024), |mut value, chunk| {
            value.extend_from_slice(&chunk);
            future::ok::<_, LoadZipError>(value)
        })
        .and_then(|value| {
            future::result(String::from_utf8(Vec::from_buf(value)))
                .from_err()
        })
}

#[derive(Debug, Fail)]
pub enum LoadZipError {
    /// Error connecting to database.
    #[fail(display = "Cannot obtain database connection: {}", _0)]
    DatabasePool(#[cause] r2d2::Error),
    /// System error.
    #[fail(display = "IO error: {}", _0)]
    Io(#[cause] std::io::Error),
    /// Error reading data from client.
    #[fail(display = "Error transferring data: {}", _0)]
    Payload(#[cause] PayloadError),
    /// Client sent invalid UTF-8.
    #[fail(display = "Invalid UTF-8: {}", _0)]
    DecodeUtf8(#[cause] std::string::FromUtf8Error),
    /// Error processing multipart.
    #[fail(display = "{}", _0)]
    Multipart(#[cause] MultipartError),
    /// Multipart field is missing content disposition.
    #[fail(display = "Field missing Content-Disposition")]
    ContentDispositionMissing,
    /// Multipart field is not a `form-data`.
    #[fail(display = "Field is not form-data")]
    FieldNotFormData,
    /// Multipart field is not named.
    #[fail(display = "Field is not named")]
    UnnamedField,
    /// Multipart includes an unknown field.
    #[fail(display = "Unknown field {:?}", _0)]
    UnknownField(String),
    /// Request is missing one of required fields.
    #[fail(display = "Incomplete request")]
    Incomplete,
    /// Tried to replace a module which doesn't exist.
    #[fail(display = "Module not found: {}", _0)]
    NoSuchModule(#[cause] FindModuleError),
    /// Error sending messages between actors.
    #[fail(display = "Problem communicating between actors: {}", _0)]
    Actor(#[cause] actix::MailboxError),
    /// Error importing a ZIP.
    #[fail(display = "Cannot import module: {}", _0)]
    Import(#[cause] ImportError),
}

impl_from! { for LoadZipError ;
    r2d2::Error => |e| LoadZipError::DatabasePool(e),
    std::io::Error => |e| LoadZipError::Io(e),
    PayloadError => |e| LoadZipError::Payload(e),
    std::string::FromUtf8Error => |e| LoadZipError::DecodeUtf8(e),
    MultipartError => |e| LoadZipError::Multipart(e),
    FindModuleError => |e| LoadZipError::NoSuchModule(e),
    actix::MailboxError => |e| LoadZipError::Actor(e),
    ImportError => |e| LoadZipError::Import(e),
}

impl ResponseError for LoadZipError {
    fn error_response(&self) -> HttpResponse {
        use self::LoadZipError::*;

        match *self {
            DatabasePool(_) | Io(_) | Actor(_) =>
                HttpResponse::InternalServerError().finish(),
            DecodeUtf8(_) | UnknownField(_) | ContentDispositionMissing
            | FieldNotFormData | UnnamedField | Incomplete | NoSuchModule(_) =>
                HttpResponse::BadRequest().body(self.to_string()),
            Payload(ref e) => e.error_response(),
            Multipart(ref e) => e.error_response(),
            Import(ref e) => e.error_response(),
        }
    }
}
