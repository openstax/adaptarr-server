use actix_web::{
    App,
    HttpMessage,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    Responder,
    http::Method,
};
use futures::{Future, future};
use uuid::Uuid;

use crate::{
    models::{
        File,
        draft::{Draft, PublicData as DraftData},
    },
    processing::ProcessDocument,
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    session::Session,
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
        .api_route("/drafts/{id}/save", Method::POST, save_draft)
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
}

type Result<T, E=Error> = std::result::Result<T, E>;

/// List current user's all drafts.
///
/// ## Method
///
/// ```
/// GET /drafts
/// ```
pub fn list_drafts(
    state: actix_web::State<State>,
    session: Session,
) -> Result<Json<Vec<DraftData>>> {
    let db = state.db.get()?;
    let drafts = Draft::all_of(&*db, session.user)?;
    Ok(Json(drafts.into_iter().map(|d| d.get_public()).collect()))
}

/// Get a draft by ID.
///
/// ## Method
///
/// ```
/// GET /drafts/:id
/// ```
pub fn get_draft(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
) -> Result<Json<DraftData>> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id, session.user)?;

    Ok(Json(draft.get_public()))
}

#[derive(Debug, Deserialize)]
pub struct DraftUpdate {
    title: String,
}

/// Update a draft.
///
/// ## Method
///
/// ```
/// PUT /drafts/:id
/// ```
pub fn update_draft(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
    update: Json<DraftUpdate>,
) -> Result<Json<DraftData>> {
    let db = state.db.get()?;
    let mut draft = Draft::by_id(&*db, *id, session.user)?;

    draft.set_title(&*db, &update.title)?;

    Ok(Json(draft.get_public()))
}

/// Delete a draft
///
/// ## Method
///
/// ```
/// DELTE /drafts/:id
/// ```
pub fn delete_draft(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id, session.user)?;

    draft.delete(&*db)?;

    Ok(HttpResponse::Ok().finish())
}

/// Save a draft.
///
/// ## Method
///
/// ```
/// POST /drafts/:id/save
/// ```
pub fn save_draft(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id, session.user)?;

    if let Err(err) = state.xref_processor.try_send(ProcessDocument {
        document: (**draft).clone(),
    }) {
        error!("Could not send document {} for processing: {}", draft.id, err);
    }

    draft.save(&*db)?;

    Ok(HttpResponse::Ok().finish())
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

#[derive(Debug, Serialize)]
pub struct FileInfo {
    name: String,
    mime: String,
}

/// List files in a draft.
///
/// ## Method
///
/// ```
/// GET /drafts/:id/files
/// ```
pub fn list_files(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
) -> Result<Json<Vec<FileInfo>>> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id, session.user)?;

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
/// ```
/// GET /drafts/:id/files/:name
/// ```
pub fn get_file(
    state: actix_web::State<State>,
    session: Session,
    path: Path<(Uuid, String)>,
) -> Result<impl Responder> {
    let (id, name) = path.into_inner();
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, id, session.user)?;

    Ok(draft.get_file(&*db, &name)?
        .stream(&state.config))
}

/// Update a file in a draft.
///
/// ## Method
///
/// ```
/// PUT /drafts/:id/files/:name
/// ```
pub fn update_file(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    session: Session,
    path: Path<(Uuid, String)>,
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    let (id, name) = path.into_inner();
    let storage = state.config.storage.path.clone();

    let db = match state.db.get() {
        Ok(db) => db,
        Err(err) => return Box::new(future::err(err.into())),
    };

    let draft = match Draft::by_id(&*db, id, session.user) {
        Ok(draft) => draft,
        Err(err) => return Box::new(future::err(err.into())),
    };

    Box::new(File::from_stream::<_, _, Error>(
            state.db.clone(),
            storage,
            req.payload(),
        )
        .and_then(move |file| {
            draft.write_file(&*db, &name, &file)
                .map_err(Into::into)
                .map(|_| HttpResponse::Ok().finish())
        }))
}

/// Delete a file from a draft.
///
/// ## Method
///
/// ```
/// DELETE /drafts/:id/files/:name
/// ```
pub fn delete_file(
    state: actix_web::State<State>,
    session: Session,
    path: Path<(Uuid, String)>,
) -> Result<HttpResponse> {
    let (id, name) = path.into_inner();
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, id, session.user)?;

    draft.delete_file(&*db, &name)?;

    Ok(HttpResponse::Ok().finish())
}

/// Get a list of all books containing the module this draft was derived from.
///
/// ## Method
///
/// ```
/// GET /modules/:id/books
/// ```
pub fn list_containing_books(
    state: actix_web::State<State>,
    session: Session,
    id: Path<Uuid>,
) -> Result<Json<Vec<Uuid>>> {
    let db = state.db.get()?;
    let draft = Draft::by_id(&*db, *id, session.user)?;

    draft.get_books(&*db)
        .map(Json)
        .map_err(Into::into)
}
