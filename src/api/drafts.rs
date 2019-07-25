use actix_web::{
    App,
    HttpMessage,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    Responder,
    http::{StatusCode, Method},
};
use failure::Fail;
use futures::{Future, future};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ApiError,
    db::{Connection, types::SlotPermission},
    models::{
        File,
        user::{User, Fields, PublicData as UserData},
        editing::{Slot, VersionData, SlotData},
        draft::{
            Draft,
            PublicData as DraftData,
            AdvanceResult as DraftAdvanceResult,
            FindDraftError,
        },
        module::PublicData as ModuleData,
    },
    permissions::ManageProcess,
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    session::Session,
    util::{FormOrJson, IfMatch},
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .api_route("/drafts", Method::GET, list_drafts)
        .resource("/drafts/{id}", |r| {
            r.get().api_with(get_draft);
            r.put().api_with(update_draft);
            r.delete().api_with(delete_draft);
        })
        .api_route("/drafts/{id}/advance", Method::POST, advance_draft)
        .resource("/drafts/{id}/comments", |r| {
            r.get().f(list_comments);
            r.post().f(add_comment);
        })
        .api_route("/drafts/{id}/files", Method::GET, list_files)
        .resource("/drafts/{id}/files/{name}", |r| {
            r.get().api_with(get_file);
            r.put().api_with_async(update_file);
            r.delete().api_with(delete_file);
        })
        .api_route("/drafts/{id}/books", Method::GET, list_containing_books)
        .api_route("/drafts/{id}/process", Method::GET, get_process_details)
        .api_route("/drafts/{id}/process/slots/{slot}", Method::PUT, assign_slot)
}

type Result<T, E=Error> = std::result::Result<T, E>;

/// List current user's all drafts.
///
/// ## Method
///
/// ```text
/// GET /drafts
/// ```
pub fn list_drafts(
    state: actix_web::State<State>,
    session: Session,
) -> Result<Json<Vec<DraftData>>> {
    let db = state.db.get()?;
    let drafts = Draft::all_of(&*db, session.user)?;

    drafts.into_iter().map(|d| d.get_public(&*db, session.user_id()))
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
        .map_err(Into::into)
}

/// Get a draft by ID.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id
/// ```
pub fn get_draft(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
) -> Result<Json<DraftData>> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id)?;
    let user = session.user(&*db)?;

    if !draft.check_access(&*db, &user)? {
        return Err(FindDraftError::NotFound.into());
    }

    draft.get_public(&*db, session.user_id()).map(Json).map_err(Into::into)
}

#[derive(Debug, Deserialize)]
pub struct DraftUpdate {
    title: String,
}

/// Update a draft.
///
/// ## Method
///
/// ```text
/// PUT /drafts/:id
/// ```
pub fn update_draft(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
    update: Json<DraftUpdate>,
) -> Result<Json<DraftData>> {
    let db = state.db.get()?;
    let mut draft = Draft::by_id_and_user(&*db, *id, session.user)?;

    if !draft.check_permission(&*db, session.user_id(), SlotPermission::Edit)? {
        return Err(InsufficientSlotPermission(SlotPermission::Edit).into());
    }

    draft.set_title(&*db, &update.title)?;

    draft.get_public(&*db, session.user_id()).map(Json).map_err(Into::into)
}

/// Delete a draft.
///
/// ## Method
///
/// ```text
/// DELETE /drafts/:id
/// ```
pub fn delete_draft(
    state: actix_web::State<State>,
    session: Session<ManageProcess>,
    id: Path<Uuid>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id)?;

    draft.delete(&*db)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

#[derive(Deserialize)]
pub struct AdvanceData {
    target: i32,
    slot: i32,
}

/// Advance a draft to the next editing step.
///
/// ## Method
///
/// ```text
/// POST /drafts/:id/advance
/// ```
pub fn advance_draft(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
    form: FormOrJson<AdvanceData>,
) -> Result<Json<AdvanceResult>> {
    let db = state.db.get()?;
    let form = form.into_inner();
    let draft = Draft::by_id_and_user(&*db, *id, session.user)?;

    draft.advance(&*db, session.user_id(), form.slot, form.target)
        .and_then(|r| Ok(match r {
            DraftAdvanceResult::Advanced(draft) => AdvanceResult::Advanced {
                draft: draft.get_public_small(),
            },
            DraftAdvanceResult::Finished(module) => AdvanceResult::Finished {
                module: module.get_public(&*db)?,
            },
        }))
        .map(Json)
        .map_err(Into::into)
}

#[derive(Serialize)]
#[serde(tag = "code")]
pub enum AdvanceResult {
    #[serde(rename = "draft:process:advanced")]
    Advanced {
        draft: DraftData,
    },
    #[serde(rename = "draft:process:finished")]
    Finished {
        module: ModuleData,
    }
}

/// Get comments on a draft.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/comments
/// ```
pub fn list_comments(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Add a comment to a draft
///
/// ## Method
///
/// ```text
/// POST /drafts/:id/comments
/// ```
pub fn add_comment(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
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
pub fn list_files(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
) -> Result<Json<Vec<FileInfo>>> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id)?;
    let user = session.user(&*db)?;

    if !draft.check_access(&*db, &user)? {
        return Err(FindDraftError::NotFound.into());
    }

    let files = draft.get_files(&*db)?
        .into_iter()
        .map(|(name, file)| FileInfo {
            name,
            mime: file.into_db().mime,
        })
        .collect();

    Ok(Json(files))
}

/// Get a file from a draft.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/files/:name
/// ```
pub fn get_file(
    state: actix_web::State<State>,
    session: Session,
    path: Path<(Uuid, String)>,
) -> Result<impl Responder> {
    let (id, name) = path.into_inner();
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, id)?;
    let user = session.user(&*db)?;

    if !draft.check_access(&*db, &user)? {
        return Err(FindDraftError::NotFound.into());
    }

    Ok(draft.get_file(&*db, &name)?
        .stream(&state.config))
}

/// Update a file in a draft.
///
/// ## Method
///
/// ```text
/// PUT /drafts/:id/files/:name
/// ```
pub fn update_file(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    session: Session,
    path: Path<(Uuid, String)>,
    if_match: IfMatch,
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    let (id, name) = path.into_inner();
    let storage = state.config.storage.path.clone();

    let db = match state.db.get() {
        Ok(db) => db,
        Err(err) => return Box::new(future::err(err.into())),
    };

    let draft = match Draft::by_id_and_user(&*db, id, session.user) {
        Ok(draft) => draft,
        Err(err) => return Box::new(future::err(err.into())),
    };

    match check_upload_permissions(&*db, &draft, session.user_id(), &name) {
        Ok(true) => (),
        Ok(false) => return Box::new(future::err(
            InsufficientSlotPermission(SlotPermission::Edit).into())),
        Err(err) => return Box::new(future::err(err)),
    }

    let mime = if name == "index.cnxml" {
        Some(&*crate::models::file::CNXML_MIME)
    } else {
        None
    };

    if !if_match.is_any() {
        let file = match draft.get_file(&*db, &name) {
            Ok(file) => file,
            Err(err) => return Box::new(future::err(err.into())),
        };

        if !if_match.test(&file.entity_tag()) {
            return Box::new(future::ok(
                HttpResponse::new(StatusCode::PRECONDITION_FAILED)));
        }
    }

    Box::new(File::from_stream::<_, _, Error>(
            state.db.clone(),
            storage,
            req.payload(),
            mime,
        )
        .and_then(move |file| {
            draft.write_file(&*db, &name, &file)
                .map_err(Into::into)
                .map(|_| HttpResponse::new(StatusCode::NO_CONTENT))
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
pub fn delete_file(
    state: actix_web::State<State>,
    session: Session,
    path: Path<(Uuid, String)>,
) -> Result<HttpResponse> {
    let (id, name) = path.into_inner();
    let db = state.db.get()?;
    let draft = Draft::by_id_and_user(&*db, id, session.user)?;

    if !draft.check_permission(&*db, session.user_id(), SlotPermission::Edit)? {
        return Err(InsufficientSlotPermission(SlotPermission::Edit).into());
    }

    draft.delete_file(&*db, &name)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

/// Get a list of all books containing the module this draft was derived from.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/books
/// ```
pub fn list_containing_books(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
) -> Result<Json<Vec<Uuid>>> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id)?;
    let user = session.user(&*db)?;

    if !draft.check_access(&*db, &user)? {
        return Err(FindDraftError::NotFound.into());
    }

    draft.get_books(&*db)
        .map(Json)
        .map_err(Into::into)
}

#[derive(Serialize)]
pub struct SlotSeating {
    #[serde(flatten)]
    slot: SlotData,
    user: Option<UserData>,
}

#[derive(Serialize)]
pub struct ProcessDetails {
    #[serde(flatten)]
    process: VersionData,
    slots: Vec<SlotSeating>,
}

/// Get details of the process this draft follows.
///
/// ## Method
///
/// ```text
/// GET /drafts/:id/process
/// ```
pub fn get_process_details(
    state: actix_web::State<State>,
    session: Session<ManageProcess>,
    id: Path<Uuid>,
) -> Result<Json<ProcessDetails>> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id)?;
    let process = draft.get_process(&*db)?;

    if !draft.check_access(&*db, &session.user(&*db)?)? {
        return Err(FindDraftError::NotFound.into());
    }

    let slots = process.get_slots(&*db)?
        .into_iter()
        .map(|slot| Ok(SlotSeating {
            slot: slot.get_public(&*db)?,
            user: slot.get_occupant(&*db, &draft)?
                .map(|user| user.get_public(Fields::empty())),
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
pub fn assign_slot(
    state: actix_web::State<State>,
    _session: Session<ManageProcess>,
    path: Path<(Uuid, i32)>,
    user: Json<i32>,
) -> Result<HttpResponse> {
    let (draft, slot) = path.into_inner();
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, draft)?;
    let slot = Slot::by_id(&*db, slot)?;
    let user = User::by_id(&*db, *user)?;

    slot.fill_with(&*db, &draft, &user)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

#[derive(ApiError, Debug, Fail)]
#[fail(display = "Missing required slot permission '{}'", _0)]
#[api(code = "draft:process:insufficient-permission", status = "FORBIDDEN")]
struct InsufficientSlotPermission(SlotPermission);
