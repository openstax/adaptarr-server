use actix_web::{
    App,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    error::ErrorInternalServerError,
};
use uuid::Uuid;

use crate::models::book::{Book, PublicData as BookData};
use super::{
    State,
    session::Session,
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/books", |r| {
            r.get().f(list_books);
            r.post().f(create_book);
        })
        .resource("/books/{id}", |r| {
            r.get().with(get_book);
            r.delete().f(delete_book);
        })
        .resource("/books/{id}/parts", |r| {
            r.get().f(book_contents);
            r.post().f(create_part);
        })
        .resource("/books/{id}/parts/{number}", |r| {
            r.get().f(get_part);
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

/// Create a new book.
///
/// ## Method
///
/// ```
/// POST /books
/// ```
pub fn create_book(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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
pub fn book_contents(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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
pub fn get_part(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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
