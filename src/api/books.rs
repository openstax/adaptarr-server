use actix_web::{App, HttpRequest, HttpResponse};

use super::State;

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/books", |r| {
            r.get().f(list_books);
            r.post().f(create_book);
        })
        .resource("/books/{id}", |r| {
            r.get().f(get_book);
            r.delete().f(delete_book);
        })
}

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
pub fn get_book(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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
