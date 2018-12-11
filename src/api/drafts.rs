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

use crate::models::draft::{Draft, PublicData as DraftData};
use super::{
    State,
    session::Session,
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .route("/drafts", Method::GET, list_drafts)
        .resource("/drafts/{id}", |r| {
            r.get().with(get_draft);
            r.delete().f(delete_draft);
        })
        .route("/drafts/{id}/save", Method::POST, save_draft)
        .resource("/drafts/{id}/comments", |r| {
            r.get().f(list_comments);
            r.post().f(add_comment);
        })
        .route("/drafts/{id}/files", Method::GET, list_files)
        .resource("/drafts/{id}/files/{name}", |r| {
            r.get().f(get_file);
            r.put().f(update_file);
            r.delete().f(delete_file);
        })
}

type Result<T> = std::result::Result<T, actix_web::error::Error>;

/// List current user's all drafts.
///
/// ## Method
///
/// ```
/// GET /drafts
/// ```
pub fn list_drafts((
    state,
    session,
): (
    actix_web::State<State>,
    Session,
)) -> Result<Json<Vec<DraftData>>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let drafts = Draft::all_of(&*db, session.user)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    Ok(Json(drafts.into_iter().map(|d| d.get_public()).collect()))
}

/// Get a draft by ID.
///
/// ## Method
///
/// ```
/// GET /drafts/:id
/// ```
pub fn get_draft((
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
    let draft = Draft::by_id(&*db, *id, session.user)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(Json(draft.get_public()))
}

/// Delete a draft
///
/// ## Method
///
/// ```
/// DELTE /drafts/:id
/// ```
pub fn delete_draft(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Save a draft.
///
/// ## Method
///
/// ```
/// POST /drafts/:id/save
/// ```
pub fn save_draft(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Get comments on a draft.
///
/// ## Method
///
/// ```
/// GET /drafts/:id/comments
/// ```
pub fn list_comments(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Add a comment to a draft
///
/// ## Method
///
/// ```
/// POST /drafts/:id/comments
/// ```
pub fn add_comment(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// List files in a draft.
///
/// ## Method
///
/// ```
/// GET /drafts/:id/files
/// ```
pub fn list_files(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Get a file from a draft.
///
/// ## Method
///
/// ```
/// GET /drafts/:id/files/:name
/// ```
pub fn get_file(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Update a file in a draft.
///
/// ## Method
///
/// ```
/// PUT /drafts/:id/files/:name
/// ```
pub fn update_file(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Delete a file from a draft.
///
/// ## Method
///
/// ```
/// DELETE /drafts/:id/files/:name
/// ```
pub fn delete_file(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}
