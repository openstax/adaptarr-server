use actix_web::{
    App,
    Form,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    error::ErrorInternalServerError,
};
use diesel::result::Error as DbError;
use uuid::Uuid;

use crate::{
    db,
    models::{
        module::{Module, FindModuleError},
        book::{Book, PublicData as BookData},
        bookpart::{BookPart, PublicData as PartData, Tree, CreatePartError},
    },
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
            r.delete().with(delete_book);
        })
        .resource("/books/{id}/parts", |r| {
            r.get().with(book_contents);
            r.post().with(create_part);
        })
        .resource("/books/{id}/parts/{number}", |r| {
            r.get().with(get_part);
            r.delete().with(delete_part);
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
pub fn delete_book((
    state,
    _session,
    id,
): (
    actix_web::State<State>,
    ElevatedSession,
    Path<Uuid>,
)) -> Result<HttpResponse> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let book = Book::by_id(&*db, *id)?;

    book.delete(&*db).map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(HttpResponse::Ok().finish())
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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum NewPart {
    Module {
        title: Option<String>,
        module: Uuid,
    },
    Group {
        title: String,
        parts: Vec<NewPart>,
    },
}

#[derive(Debug, Deserialize)]
pub struct NewPartRoot {
    #[serde(flatten)]
    part: NewPart,
    parent: i32,
    index: i32,
}

#[derive(Debug, Serialize)]
pub struct NewPartData {
    number: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    parts: Option<Vec<NewPartData>>,
}

/// Create a new part.
///
/// ## Method
///
/// ```
/// POST /books/:id/parts
/// ```
pub fn create_part((
    state,
    _session,
    book,
    part,
): (
    actix_web::State<State>,
    ElevatedSession,
    Path<Uuid>,
    Json<NewPartRoot>,
)) -> Result<Json<NewPartData>> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let NewPartRoot { part, parent, index } = part.into_inner();
    let parent = BookPart::by_id(&*db, *book, parent)?;

    println!("DATA: {:?}", part);

    use diesel::Connection;
    let data = db.transaction(|| {
        create_part_inner(&*db, &parent, index, part)
    }).map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(Json(data))
}

/// Recursively create parts.
fn create_part_inner(
    dbconn: &db::Connection,
    parent: &BookPart,
    index: i32,
    template: NewPart,
) -> std::result::Result<NewPartData, RealizeTemplateError> {
    match template {
        NewPart::Module { title, module } => {
            let module = Module::by_id(dbconn, module)?;

            let part = parent.insert_module(
                dbconn,
                index,
                title.as_ref().map_or(module.name.as_str(), String::as_str),
                &module,
            )?;

            Ok(NewPartData {
                number: part.id,
                parts: None,
            })
        }
        NewPart::Group { title, parts } => {
            let group = parent.create_group(dbconn, index, title.as_str())?;

            Ok(NewPartData {
                number: group.id,
                parts: parts.into_iter()
                    .enumerate()
                    .map(|(index, part)| {
                        create_part_inner(dbconn, &group, index as i32, part)
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map(Some)?,
            })
        }
    }
}

#[derive(Debug, Fail)]
enum RealizeTemplateError {
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    #[fail(display = "Module not found: {}", _0)]
    ModuleNotFound(#[cause] FindModuleError),
    #[fail(display = "Part could not be created: {}", _0)]
    PartCreation(CreatePartError),
}

impl_from! { for RealizeTemplateError ;
    DbError => |e| RealizeTemplateError::Database(e),
    FindModuleError => |e| RealizeTemplateError::ModuleNotFound(e),
    CreatePartError => |e| RealizeTemplateError::PartCreation(e),
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
pub fn delete_part((
    state,
    _session,
    path,
): (
    actix_web::State<State>,
    Session,
    Path<(Uuid, i32)>,
)) -> Result<HttpResponse> {
    let db = state.db.get()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;
    let (book, id) = path.into_inner();

    BookPart::by_id(&*db, book, id)?
        .delete(&*db)
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(HttpResponse::Ok().finish())
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
