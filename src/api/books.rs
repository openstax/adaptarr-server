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
