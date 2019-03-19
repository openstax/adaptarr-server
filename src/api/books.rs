use actix_web::{
    App,
    HttpMessage,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    pred,
};
use diesel::result::Error as DbError;
use futures::{Future, Stream, future};
use std::io::Write;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{
    db,
    models::{
        module::{Module, FindModuleError},
        book::{Book, PublicData as BookData},
        bookpart::{
            BookPart,
            CreatePartError,
            PublicData as PartData,
            ReparentPartError,
            Tree,
        },
    },
    multipart::Multipart,
    import::{ImportBook, ReplaceBook}
    permissions::EditBook,
};
use super::{
    Error,
    RouteExt,
    State,
    session::Session,
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/books", |r| {
            r.get().api_with(list_books);
            r.post()
                .filter(pred::Header("Content-Type", "application/json"))
                .api_with(create_book);
            r.post().api_with_async(create_book_from_zip);
        })
        .resource("/books/{id}", |r| {
            r.get().api_with(get_book);
            r.put()
                .filter(pred::Header("Content-Type", "application/json"))
                .api_with(update_book);
            r.put().api_with_async(replace_book);
            r.delete().api_with(delete_book);
        })
        .resource("/books/{id}/parts", |r| {
            r.get().api_with(book_contents);
            r.post().api_with(create_part);
        })
        .resource("/books/{id}/parts/{number}", |r| {
            r.get().api_with(get_part);
            r.delete().api_with(delete_part);
            r.put().api_with(update_part);
        })
}

type Result<T, E=Error> = std::result::Result<T, E>;

/// List all books.
///
/// ## Method
///
/// ```
/// GET /books
/// ```
pub fn list_books(
    state: actix_web::State<State>,
    _session: Session,
) -> Result<Json<Vec<BookData>>> {
    let db = state.db.get()?;
    let books = Book::all(&*db)?;
    Ok(Json(books.into_iter()
        .map(|book| book.get_public())
        .collect()))
}

#[derive(Debug, Deserialize)]
pub struct NewBook {
    title: String,
}

/// Create a new book.
///
/// ## Method
///
/// ```
/// POST /books
/// Content-Type: application/json
/// ```
pub fn create_book(
    state: actix_web::State<State>,
    _session: Session<EditBook>,
    form: Json<NewBook>,
) -> Result<Json<BookData>> {
    let db = state.db.get()?;
    let book = Book::create(&*db, &form.title)?;
    Ok(Json(book.get_public()))
}

pub struct NewBookZip {
    title: String,
    file: NamedTempFile,
}

from_multipart! {
    multipart NewBookZip via _NewBookZipImpl {
        title: String,
        file: NamedTempFile,
    }
}

/// Create a new book, populating it with contents of a ZIP archive.
///
/// ## Method
///
/// ```
/// POST /books
/// Content-Type: multipart/form-data
/// ```
pub fn create_book_from_zip(
    state: actix_web::State<State>,
    _session: Session<EditBook>,
    data: Multipart<NewBookZip>,
) -> impl Future<Item = Json<BookData>, Error = Error> {
    let NewBookZip { title, file } = data.into_inner();
    state.importer.send(ImportBook { title, file })
        .from_err()
        .and_then(|r| future::result(r).from_err())
        .map(|book| Json(book.get_public()))
}

/// Get a book by ID.
///
/// ## Method
///
/// ```
/// GET /books/:id
/// ```
pub fn get_book(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<Uuid>,
) -> Result<Json<BookData>> {
    let db = state.db.get()?;
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
pub fn update_book(
    state: actix_web::State<State>,
    _session: Session<EditBook>,
    id: Path<Uuid>,
    change: Json<BookChange>,
) -> Result<Json<BookData>> {
    let db = state.db.get()?;
    let mut book = Book::by_id(&*db, *id)?;

    book.set_title(&*db, change.into_inner().title)?;

    Ok(Json(book.get_public()))
}

pub fn replace_book(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    _session: Session<EditBook>,
    id: Path<Uuid>,
) -> impl Future<Item = Json<BookData>, Error = Error> {
    future::result(
        state.db.get()
            .map_err(Into::into)
            .and_then(|db| Book::by_id(&*db, *id)
                .map_err(Into::into)))
        .and_then(|book| future::result(
            NamedTempFile::new()
                .map_err(Into::into)
                .map(|file| (book, file))))
        .and_then(move |(book, file)| {
            req.payload()
                .from_err()
                .fold(file, |mut file, chunk| {
                    match file.write_all(chunk.as_ref()) {
                        Ok(_) => future::ok(file),
                        Err(e) => future::err(e),
                    }
                })
                .map(|file| (book, file))
        })
        .and_then(move |(book, file)| {
            state.importer.send(ReplaceBook { book, file })
                .from_err()
        })
        .and_then(|r| future::result(r).from_err())
        .map(|book| Json(book.get_public()))
}

/// Delete a book by ID.
///
/// ## Method
///
/// ```
/// DELETE /books/:id
/// ```
pub fn delete_book(
    state: actix_web::State<State>,
    _session: Session<EditBook>,
    id: Path<Uuid>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let book = Book::by_id(&*db, *id)?;

    book.delete(&*db)?;

    Ok(HttpResponse::Ok().finish())
}

/// Get book's contents as a tree.
///
/// ## Method
///
/// ```
/// GET /books/:id/parts
/// ```
pub fn book_contents(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<Uuid>,
) -> Result<Json<Tree>> {
    let db = state.db.get()?;
    let book = BookPart::by_id(&*db, *id, 0)?;

    book.get_tree(&*db)
        .map_err(Into::into)
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
        #[serde(default)]
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
pub fn create_part(
    state: actix_web::State<State>,
    _session: Session<EditBook>,
    book: Path<Uuid>,
    part: Json<NewPartRoot>,
) -> Result<Json<NewPartData>> {
    let db = state.db.get()?;
    let NewPartRoot { part, parent, index } = part.into_inner();
    let parent = BookPart::by_id(&*db, *book, parent)?;

    println!("DATA: {:?}", part);

    use diesel::Connection;
    let data = db.transaction(|| {
        create_part_inner(&*db, &parent, index, part)
    })?;

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
                title.as_ref().map_or(module.title.as_str(), String::as_str),
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

#[derive(ApiError, Debug, Fail)]
enum RealizeTemplateError {
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    #[fail(display = "Module not found: {}", _0)]
    #[api(code = "module:not-found", status = "NOT_FOUND")]
    ModuleNotFound(#[cause] FindModuleError),
    #[fail(display = "Part could not be created: {}", _0)]
    PartCreation(#[cause] CreatePartError),
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
pub fn get_part(
    state: actix_web::State<State>,
    _session: Session,
    path: Path<(Uuid, i32)>,
) -> Result<Json<PartData>> {
    let db = state.db.get()?;
    let (book, id) = path.into_inner();
    let part = BookPart::by_id(&*db, book, id)?;

    part.get_public(&*db)
        .map_err(Into::into)
        .map(Json)
}

/// Delete a part from a book.
///
/// ## Method
///
/// ```
/// DELETE /book/:id/parts/:number
/// ```
pub fn delete_part(
    state: actix_web::State<State>,
    _session: Session<EditBook>,
    path: Path<(Uuid, i32)>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let (book, id) = path.into_inner();

    BookPart::by_id(&*db, book, id)?
        .delete(&*db)?;

    Ok(HttpResponse::Ok().finish())
}

#[derive(Debug, Deserialize)]
pub struct PartUpdate {
    title: Option<String>,
    #[serde(flatten)]
    location: Option<PartLocation>,
}

#[derive(Debug, Deserialize)]
pub struct PartLocation {
    parent: i32,
    index: i32,
}

/// Update a book part.
///
/// ## Method
///
/// ```
/// PUT /book/:id/parts/:number
/// ```
pub fn update_part(
    state: actix_web::State<State>,
    _session: Session<EditBook>,
    path: Path<(Uuid, i32)>,
    update: Json<PartUpdate>,
) -> Result<HttpResponse> {
    let db = state.db.get()?;
    let (book, id) = path.into_inner();
    let mut part = BookPart::by_id(&*db, book, id)?;
    let parent = update.location
        .as_ref()
        .map_or(Ok(None), |location| {
            BookPart::by_id(&*db, book, location.parent)
                .map(|part| Some((part, location.index)))
        })?;

    let dbconn = &*db;
    use diesel::Connection;
    dbconn.transaction::<_, ReparentPartError, _>(move || {
        if let Some(ref title) = update.title {
            part.set_title(dbconn, &title)?;
        }

        if let Some((parent, index)) = parent {
            part.reparent(dbconn, &parent, index)?;
        }

        Ok(())
    })?;

    Ok(HttpResponse::Ok().finish())
}
