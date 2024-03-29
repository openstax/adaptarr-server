use actix::Addr;
use actix_web::{
    HttpRequest,
    HttpResponse,
    Responder,
    web::{self, Data, Payload, Path, Json, ServiceConfig},
};
use adaptarr_error::Error;
use adaptarr_models::{
    CNXML_MIME,
    Draft,
    File,
    Model,
    Module,
    Team,
    User,
    XrefTarget,
    editing::Process,
    permissions::{EditModule, ManageProcess, PermissionBits, TeamPermissions},
    processing::{TargetProcessor, import::{Importer, ImportModule, ReplaceModule}},
};
use adaptarr_web::{
    ContentType,
    Created,
    Database,
    FileExt,
    FormOrJson,
    Session,
    TeamScoped,
    multipart::{FromMultipart, FromStrField, Multipart},
};
use futures::{Future, Stream, future};
use serde::{Deserialize, Serialize};
use std::io::Write;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .service(web::resource("/modules")
            .route(web::get().to(list_modules))
            .route(web::post()
                .guard(ContentType::from_mime(&mime::APPLICATION_JSON))
                .to(create_module))
            .route(web::post()
                .to_async(create_module_from_zip))
        )
        .service(web::resource("/modules/{id}")
            .route(web::get().to(get_module))
            .route(web::post().to(begin_process))
            .route(web::put().to_async(replace_module))
            .route(web::delete().to(delete_module))
        )
        .service(web::resource("/modules/{id}/comments")
            .route(web::get().to(list_comments))
            .route(web::post().to(add_comment))
        )
        .route("/modules/{id}/files", web::get().to(list_files))
        .route("/modules/{id}/files/{name}", web::get().to(get_file))
        .route("/modules/{id}/xref-targets", web::get().to(list_xref_targets))
        .route("/modules/{id}/books", web::get().to(list_containing_books))
    ;
}

/// Get list of all modules.
///
/// ## Method
///
/// ```text
/// Get /modules
/// ```
fn list_modules(db: Database, session: Session)
-> Result<Json<Vec<<Module as Model>::Public>>> {
    let modules = if session.is_elevated {
        Module::all(&db)?
    } else {
        let user = session.user(&db)?;
        let teams = user.get_team_ids(&db)?;

        Module::by_team(&db, &teams)?
    };

    Ok(Json(modules.get_public_full(&db, &())?))
}

#[derive(Deserialize)]
struct NewModule {
    title: String,
    team: i32,
    language: String,
}

/// Create a new empty module.
///
/// ## Method
///
/// ```text
/// POST /modules
/// Content-Type: application/json
/// ```
fn create_module(
    req: HttpRequest,
    db: Database,
    session: Session,
    data: Json<NewModule>,
) -> Result<Created<String, Json<<Module as Model>::Public>>> {
    let team = Team::by_id(&db, data.team)?;

    if !session.is_elevated {
        team.get_member(&db, &session.user(&db)?)?
            .permissions()
            .require(TeamPermissions::EDIT_MODULE)?;
    }

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

    let storage_path = &adaptarr_models::Config::global().storage.path;
    let index = File::from_data(&db, storage_path, &content, Some(CNXML_MIME))?;

    let module = Module::create::<&str, _>(
        &db, &team, &data.title, &data.language, index, std::iter::empty())?;

    TargetProcessor::process(module.clone());

    let public = module.get_public_full(&db, &())?;

    let location = format!("{}/api/v1/modules/{}",
        req.app_config().host(), module.id());

    Ok(Created(location, Json(public)))
}

#[derive(FromMultipart)]
struct NewModuleZip {
    title: String,
    team: FromStrField<i32>,
    file: NamedTempFile,
}

/// Create a new module, populating it with contents of a ZIP archive.
///
/// ## Method
///
/// ```text
/// POST /modules
/// Content-Type: multipart/form-data
/// ```
fn create_module_from_zip(
    req: HttpRequest,
    db: Database,
    importer: Data<Addr<Importer>>,
    session: Session,
    data: Multipart<NewModuleZip>,
) -> Box<dyn Future<Item = Created<String, Json<<Module as Model>::Public>>, Error = Error>> {
    let NewModuleZip { title, team, file } = data.into_inner();

    let team = match Team::by_id(&db, *team) {
        Ok(team) => team,
        Err(err) => return Box::new(future::err(err.into())),
    };

    match session.user(&db)
        .map_err(Error::from)
        .and_then(|user| {
            if session.is_elevated {
                Ok(TeamPermissions::all())
            } else {
                team.get_member(&db, &user)
                    .map(|tm| tm.permissions())
                    .map_err(Error::from)
            }
        })
        .and_then(|permissions| {
            permissions
                .require(TeamPermissions::EDIT_MODULE)
                .map_err(Error::from)
        })
    {
        Ok(()) => (),
        Err(err) => return Box::new(future::err(err)),
    }

    Box::new(importer.send(ImportModule {
        team,
        title,
        file,
        actor: session.user_id().into(),
    })
        .from_err::<Error>()
        .and_then(|r| future::result(r).from_err())
        .and_then(move |module| -> Result<_> {
            Ok(module.get_public_full(&db, &())?)
        })
        .map(move |p| {
            let location = format!("{}/api/v1/modules/{}",
                req.app_config().host(), p.id);
            Created(location, Json(p))
        }))
}

/// Get a module by ID.
///
/// ## Method
///
/// ```text
/// GET /modules/:id
/// ```
fn get_module(db: Database, scope: TeamScoped<Module>)
-> Result<Json<<Module as Model>::Public>> {
    Ok(Json(scope.resource().get_public_full(&db, &())?))
}

#[derive(Deserialize)]
struct BeginProcess {
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
fn begin_process(
    req: HttpRequest,
    db: Database,
    scope: TeamScoped<Module, ManageProcess>,
    session: Session,
    data: FormOrJson<BeginProcess>,
) -> Result<Created<String, Json<<Draft as Model>::Public>>> {
    let data = data.into_inner();
    let module = scope.resource();
    let process = Process::by_id(&db, data.process)?;
    let version = process.get_current(&db)?;

    let slots = data.slots.into_iter()
        .map(|(slot, user)| Ok((
            version.get_slot(&db, slot)?,
            User::by_id(&db, user)?,
        )))
        .collect::<Result<Vec<_>>>()?;

    let draft = module.begin_process(&db, &version, slots)?;
    let public = draft.get_public_full(&db, &session.user_id())?;
    let location = format!("{}/api/v1/drafts/{}",
        req.app_config().host(), draft.id());

    Ok(Created(location, Json(public)))
}

/// Replace module with contents of a ZIP archive.
///
/// ## Method
///
/// ```text
/// PUT /modules/:id
/// ```
fn replace_module(
    db: Database,
    importer: Data<Addr<Importer>>,
    scope: TeamScoped<Module, EditModule>,
    session: Session,
    payload: Payload,
) -> Box<dyn Future<Item = Json<<Module as Model>::Public>, Error = Error>> {
    let module = scope.into_resource();
    let file = match NamedTempFile::new() {
        Ok(file) => file,
        Err(err) => return Box::new(future::err(err.into())),
    };

    Box::new(payload
        .from_err::<Error>()
        .fold(file, |mut file, chunk| match file.write_all(chunk.as_ref()) {
            Ok(_) => future::ok(file),
            Err(err) => future::err(err),
        })
        .and_then(move |file| importer.send(ReplaceModule {
            module,
            file,
            actor: session.user_id().into(),
        }).from_err())
        .and_then(|r| future::result(r).from_err())
        .and_then(move |module| {
            module.get_public_full(&db, &()).map_err(Error::from)
        })
        .map(Json))
}

/// Delete a module
///
/// ## Method
///
/// ```text
/// DELTE /modules/:id
/// ```
fn delete_module() -> HttpResponse {
    unimplemented!()
}

/// Get comments on a module.
///
/// ## Method
///
/// ```text
/// GET /modules/:id/comments
/// ```
fn list_comments() -> HttpResponse {
    unimplemented!()
}

/// Add a comment to a module
///
/// ## Method
///
/// ```text
/// POST /modules/:id/comments
/// ```
fn add_comment() -> HttpResponse {
    unimplemented!()
}

#[derive(Debug, Serialize)]
struct FileInfo {
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
fn list_files(db: Database, scope: TeamScoped<Module>)
-> Result<Json<Vec<FileInfo>>> {
    Ok(Json(scope.resource()
        .get_files(&db)?
        .into_iter()
        .map(|(name, file)| FileInfo {
            name,
            mime: file.into_db().mime,
        })
        .collect()))
}

/// Get a file from a module.
///
/// ## Method
///
/// ```text
/// GET /modules/:id/files/:name
/// ```
fn get_file(db: Database, scope: TeamScoped<Module>, path: Path<(Uuid, String)>)
-> Result<impl Responder> {
    let (_, name) = path.into_inner();

    let storage_path = &adaptarr_models::Config::global().storage.path;
    Ok(scope.resource().get_file(&db, &name)?.stream(storage_path))
}

/// Get a list of all possible cross-reference targets within a module.
///
/// ## Method
///
/// ```text
/// GET /modules/:id/xref-targets
/// ```
fn list_xref_targets(db: Database, scope: TeamScoped<Module>)
-> Result<Json<Vec<<XrefTarget as Model>::Public>>> {
    Ok(Json(scope.resource().xref_targets(&db)?.get_public()))
}

/// Get a list of all books containing this module in them.
///
/// ## Method
///
/// ```text
/// GET /modules/:id/books
/// ```
fn list_containing_books(db: Database, scope: TeamScoped<Module>)
-> Result<Json<Vec<Uuid>>> {
    Ok(Json(scope.resource().get_books(&db)?))
}
