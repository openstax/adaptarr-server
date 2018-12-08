use actix_web::{
    App,
    Form,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    error::ErrorInternalServerError,
};
use uuid::Uuid;

use crate::models::{
    book::{Book, PublicData as BookData},
    bookpart::{BookPart, PublicData as PartData, Tree},
};
use super::{
    State,
    session::{ElevatedSession, Session},
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/books", |r| {
            r.get().f(list_books);
            r.post().with(create_book);
        })
        .resource("/books/{id}", |r| {
            r.get().with(get_book);
            r.put().with(update_book);
            r.delete().f(delete_book);
        })
        .resource("/books/{id}/parts", |r| {
            r.get().with(book_contents);
            r.post().f(create_part);
        })
        .resource("/books/{id}/parts/{number}", |r| {
            r.get().with(get_part);
            r.delete().f(delete_part);
            r.put().f(update_part);
        })
}

type Result<T> = std::result::Result<T, actix_web::Error>;

/// List all books.
///
/// ## Method
///
/// ```
/// GET /books
/// ```
pub fn list_books(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

#[derive(Debug, Deserialize)]
pub struct BookForm {
    title: String,
}

/// Create a new book.
///
/// ## Method
///
/// ```
/// POST /books
/// ```
pub fn create_book((
    state,
    _session,
    form,
): (
    actix_web::State<State>,
    ElevatedSession,
    Form<BookForm>,
)) -> Result<Json<BookData>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let book = Book::create(&*db, &form.title)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    Ok(Json(book.get_public()))
}

/// Get a book by ID.
///
/// ## Method
///
/// ```
/// GET /books/:id
/// ```
pub fn get_book((
    state,
    _session,
    id,
): (
    actix_web::State<State>,
    Session,
    Path<Uuid>,
)) -> Result<Json<BookData>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let book = Book::by_id(&*db, *id)?;

    Ok(Json(book.get_public()))
}

#[derive(Debug, Deserialize)]
pub struct BookChange {
    title: String,
}

/// Update books metadata.
///
/// ## Method
///
/// ```
/// PUT /books/:id
/// ```
pub fn update_book((
    state,
    _session,
    id,
    change,
): (
    actix_web::State<State>,
    ElevatedSession,
    Path<Uuid>,
    Json<BookChange>,
)) -> Result<Json<BookData>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let mut book = Book::by_id(&*db, *id)?;

    book.set_title(&*db, change.into_inner().title)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(Json(book.get_public()))
}

/// Delete a book by ID.
///
/// ## Method
///
/// ```
/// DELETE /books/:id
/// ```
pub fn delete_book(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Get book's contents as a tree.
///
/// ## Method
///
/// ```
/// GET /books/:id/parts
/// ```
pub fn book_contents((
    state,
    _session,
    id,
): (
    actix_web::State<State>,
    Session,
    Path<Uuid>,
)) -> Result<Json<Tree>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let book = BookPart::by_id(&*db, *id, 0)?;

    book.get_tree(&*db)
        .map_err(|e| ErrorInternalServerError(e.to_string()))
        .map(Json)
}

/// Create a new part.
///
/// ## Method
///
/// ```
/// POST /books/:id/parts
/// ```
pub fn create_part(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Inspect a single part of a book.
///
/// ## Method
///
/// ```
/// GET /book/:id/parts/:number
/// ```
pub fn get_part((
    state,
    _session,
    path,
): (
    actix_web::State<State>,
    Session,
    Path<(Uuid, i32)>,
)) -> Result<Json<PartData>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let (book, id) = path.into_inner();
    let part = BookPart::by_id(&*db, book, id)?;

    part.get_public(&*db)
        .map_err(|e| ErrorInternalServerError(e.to_string()))
        .map(Json)
}

/// Delete a part from a book.
///
/// ## Method
///
/// ```
/// DELETE /book/:ids/parts/:number
/// ```
pub fn delete_part(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}

/// Update a book part.
///
/// ## Method
///
/// ```
/// PUT /book/:ids/parts/:number
/// ```
pub fn update_part(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
}
