use actix_web::{
    HttpResponse,
    Responder,
    http::{StatusCode, header::ETAG},
    web::{self, Data, Json, Payload, Path, ServiceConfig},
};
use adaptarr_error::{ApiError, Error};
use adaptarr_models::{
    CNXML_MIME,
    AdvanceResult,
    Draft,
    File,
    FindModelError,
    Model,
    Module,
    User,
    UserFields,
    db::{Connection, Pool, types::SlotPermission},
    editing::{Version, Slot},
    permissions::ManageProcess,
};
use adaptarr_util::futures::void;
use adaptarr_web::{Database, FileExt, FormOrJson, Session, etag::IfMatch};
use failure::Fail;
use futures::{Future, Stream, future};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .route("/drafts", web::get().to(list_drafts))
        .service(web::resource("/drafts/{id}")
            .route(web::get().to(get_draft))
            .route(web::put().to(update_draft))
            .route(web::delete().to(delete_draft))
        )
        .route("/drafts/{id}/advance", web::post().to(advance_draft))
        .service(web::resource("/drafts/{id}/comments")
            .route(web::get().to(list_comments))
            .route(web::post().to(add_comment))
        )
        .route("/drafts/{id}/files", web::get().to(list_files))
        .service(web::resource("/drafts/{id}/files/{name}")
            .route(web::get().to(get_file))
            .route(web::put().to_async(update_file))
            .route(web::delete().to(delete_file))
        )
        .route("/drafts/{id}/books", web::get().to(list_containing_books))
        .route("/drafts/{id}/process", web::get().to(get_process_details))
        .route("/drafts/{id}/process/slots/{slot}", web::put().to(assign_slot))
    ;
}

/// List current user's all drafts.
///
/// ## Method
///
/// ```text
/// GET /drafts
/// ```
fn list_drafts(db: Database, session: Session)
-> Result<Json<Vec<<Draft as Model>::Public>>> {
    Ok(Json(Draft::all_of(&db, session.user)?
        .get_public_full(&db, session.user_id())?))
}

/// Get a draft by ID.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id
/// ```
fn get_draft(db: Database, session: Session, id: Path<Uuid>)
-> Result<Json<<Draft as Model>::Public>> {
    let draft = Draft::by_id(&db, *id)?;
    let user = session.user(&db)?;

    if !draft.check_access(&db, &user)? {
        return Err(FindModelError::<Draft>::not_found().into());
    }

    Ok(Json(draft.get_public_full(&db, user.id())?))
}

#[derive(Deserialize)]
struct DraftUpdate {
    title: String,
}

/// Update a draft.
///
/// ## Method
///
/// ```text
/// PUT /drafts/:id
/// ```
fn update_draft(
    db: Database,
    session: Session,
    id: Path<Uuid>,
    update: Json<DraftUpdate>,
) -> Result<Json<<Draft as Model>::Public>> {
    let mut draft = Draft::by_id_and_user(&db, *id, session.user)?;

    if !draft.check_permission(&*db, session.user_id(), SlotPermission::Edit)? {
        return Err(InsufficientSlotPermission(SlotPermission::Edit).into());
    }

    draft.set_title(&db, &update.title)?;

    Ok(Json(draft.get_public_full(&db, session.user_id())?))
}

/// Delete a draft.
///
/// ## Method
///
/// ```text
/// DELETE /drafts/:id
/// ```
fn delete_draft(db: Database, _: Session<ManageProcess>, id: Path<Uuid>)
-> Result<HttpResponse> {
    Draft::by_id(&db, *id)?.delete(&db)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

#[derive(Deserialize)]
struct Advance {
    target: i32,
    slot: i32,
}

#[derive(Serialize)]
#[serde(tag = "code")]
enum AdvanceData {
    #[serde(rename = "draft:process:advanced")]
    Advanced {
        draft: <Draft as Model>::Public,
    },
    #[serde(rename = "draft:process:finished")]
    Finished {
        module: <Module as Model>::Public,
    }
}

/// Advance a draft to the next editing step.
///
/// ## Method
///
/// ```text
/// POST /drafts/:id/advance
/// ```
fn advance_draft(
    db: Database,
    session: Session,
    id: Path<Uuid>,
    form: FormOrJson<Advance>,
) -> Result<Json<AdvanceData>> {
    let Advance { target, slot } = form.into_inner();
    let draft = Draft::by_id_and_user(&db, *id, session.user)?;

    match draft.advance(&db, session.user, slot, target)? {
        AdvanceResult::Advanced(draft) => Ok(Json(AdvanceData::Advanced {
            draft: draft.get_public(),
        })),
        AdvanceResult::Finished(module) => Ok(Json(AdvanceData::Finished {
            module: module.get_public(),
        })),
    }
}

/// Get comments on a draft.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/comments
/// ```
fn list_comments() -> HttpResponse {
    unimplemented!()
}

/// Add a comment to a draft
///
/// ## Method
///
/// ```text
/// POST /drafts/:id/comments
/// ```
fn add_comment() -> HttpResponse {
    unimplemented!()
}

#[derive(Serialize)]
struct FileInfo {
    name: String,
    mime: String,
}

/// List files in a draft.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/files
/// ```
fn list_files(db: Database, session: Session, id: Path<Uuid>)
-> Result<Json<Vec<FileInfo>>> {
    let draft = Draft::by_id(&db, *id)?;
    let user = session.user(&db)?;

    if !draft.check_access(&db, &user)? {
        return Err(FindModelError::<Draft>::not_found().into());
    }

    Ok(Json(draft.get_files(&db)?
        .into_iter()
        .map(|(name, file)| FileInfo {
            name,
            mime: file.into_db().mime,
        })
        .collect()))
}

/// Get a file from a draft.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/files/:name
/// ```
fn get_file(db: Database, session: Session, path: Path<(Uuid, String)>)
-> Result<impl Responder> {
    let (id, name) = path.into_inner();
    let draft = Draft::by_id(&db, id)?;
    let user = session.user(&db)?;

    if !draft.check_access(&*db, &user)? {
        return Err(FindModelError::<Draft>::not_found().into());
    }

    let storage_path = &adaptarr_models::Config::global().storage.path;
    Ok(draft.get_file(&db, &name)?.stream(storage_path))
}

/// Update a file in a draft.
///
/// ## Method
///
/// ```text
/// PUT /drafts/:id/files/:name
/// ```
fn update_file(
    db: Database,
    pool: Data<Pool>,
    session: Session,
    path: Path<(Uuid, String)>,
    if_match: IfMatch,
    payload: Payload,
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    let (id, name) = path.into_inner();

    let draft = match Draft::by_id_and_user(&db, id, session.user) {
        Ok(draft) => draft,
        Err(err) => return Box::new(future::err(err.into())),
    };

    match check_upload_permissions(&db, &draft, session.user_id(), &name) {
        Ok(true) => (),
        Ok(false) => return Box::new(future::err(
            InsufficientSlotPermission(SlotPermission::Edit).into())),
        Err(err) => return Box::new(future::err(err)),
    }

    let mime = if name == "index.cnxml" {
        Some(CNXML_MIME)
    } else {
        None
    };

    if !if_match.is_any() {
        let file = match draft.get_file(&*db, &name) {
            Ok(file) => file,
            Err(err) => return Box::new(future::err(err.into())),
        };

        if !if_match.test(&file.entity_tag()) {
            return Box::new(payload.from_err()
                .forward(void::<_, Error>())
                .map(|_| HttpResponse::new(StatusCode::PRECONDITION_FAILED)));
        }
    }

    let storage_path = &adaptarr_models::Config::global().storage.path;

    Box::new(File::from_stream::<_, _, _, _>(
            (*pool).clone(),
            storage_path,
            payload,
            mime,
        )
        .and_then(move |file| {
            draft.write_file(&db, &name, &file)
                .map_err(Into::into)
                .map(|_|
                    HttpResponse::NoContent()
                        .header(ETAG, file.entity_tag())
                        .finish()
                )
        }))
}

fn check_upload_permissions(db: &Connection, draft: &Draft, user: i32, file: &str)
-> Result<bool, Error> {
    if draft.check_permission(db, user, SlotPermission::Edit)? {
        return Ok(true);
    }

    if file == "index.cnxml" {
        if draft.check_permission(db, user, SlotPermission::ProposeChanges)? {
            return Ok(true);
        }
        if draft.check_permission(db, user, SlotPermission::AcceptChanges)? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Delete a file from a draft.
///
/// ## Method
///
/// ```text
/// DELETE /drafts/:id/files/:name
/// ```
fn delete_file(
    db: Database,
    session: Session,
    path: Path<(Uuid, String)>,
) -> Result<HttpResponse> {
    let (id, name) = path.into_inner();
    let draft = Draft::by_id_and_user(&db, id, session.user)?;

    if !draft.check_permission(&db, session.user, SlotPermission::Edit)? {
        return Err(InsufficientSlotPermission(SlotPermission::Edit).into());
    }

    draft.delete_file(&db, &name)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

/// Get a list of all books containing the module this draft was derived from.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/books
/// ```
fn list_containing_books(db: Database, session: Session, id: Path<Uuid>)
-> Result<Json<Vec<Uuid>>> {
    let draft = Draft::by_id(&db, *id)?;
    let user = session.user(&db)?;

    if !draft.check_access(&db, &user)? {
        return Err(FindModelError::<Draft>::not_found().into());
    }

    Ok(Json(draft.get_books(&db)?))
}

#[derive(Serialize)]
struct SlotSeating {
    #[serde(flatten)]
    slot: <Slot as Model>::Public,
    user: Option<<User as Model>::Public>,
}

#[derive(Serialize)]
struct ProcessDetails {
    #[serde(flatten)]
    process: <Version as Model>::Public,
    slots: Vec<SlotSeating>,
}

/// Get details of the process this draft follows.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/process
/// ```
fn get_process_details(
    db: Database,
    session: Session<ManageProcess>,
    id: Path<Uuid>,
) -> Result<Json<ProcessDetails>> {
    let draft = Draft::by_id(&db, *id)?;

    if !draft.check_access(&db, &session.user(&db)?)? {
        return Err(FindModelError::<Draft>::not_found().into());
    }

    let process = draft.get_process(&db)?;

    let slots = process.get_slots(&db)?
        .into_iter()
        .map(|slot| Ok(SlotSeating {
            slot: slot.get_public_full(&db, ())?,
            user: slot.get_occupant(&db, &draft)?
                .map(|user| user.get_public_full(&db, UserFields::empty()))
                .transpose()?
        }))
        .collect::<Result<Vec<_>>>()?;

    Ok(Json(ProcessDetails {
        process: process.get_public(),
        slots,
    }))
}

/// Assign a specific user to a slot.
///
/// ## Method
///
/// ```text
/// PUT /drafts/:id/process/slots/:slot
/// ```
fn assign_slot(
    db: Database,
    _: Session<ManageProcess>,
    path: Path<(Uuid, i32)>,
    user: Json<i32>,
) -> Result<HttpResponse> {
    let (draft_id, slot_id) = path.into_inner();
    let draft = Draft::by_id(&db, draft_id)?;
    let slot = Slot::by_id(&db, slot_id)?;
    let user = User::by_id(&db, *user)?;

    slot.fill_with(&db, &draft, &user)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

#[derive(ApiError, Debug, Fail)]
#[fail(display = "Missing required slot permission '{}'", _0)]
#[api(code = "draft:process:insufficient-permission", status = "FORBIDDEN")]
struct InsufficientSlotPermission(SlotPermission);
