use actix::Addr;
use actix_web::{
    HttpRequest,
    HttpResponse,
    http::StatusCode,
    web::{self, Data, Json, Path, Payload, ServiceConfig},
};
use adaptarr_error::Error;
use adaptarr_models::{
    Book,
    BookPart,
    FindModelError,
    Model,
    NewTree,
    Team,
    TeamResource,
    Tree,
    permissions::{EditBook, PermissionBits, TeamPermissions, SystemPermissions},
    processing::import::{Importer, ImportBook, ReplaceBook},
};
use adaptarr_web::{
    ContentType,
    Created,
    Database,
    Session,
    TeamScoped,
    multipart::{FromMultipart, FromStrField, Multipart},
};
use diesel::Connection as _;
use futures::{Future, Stream, future};
use serde::Deserialize;
use std::io::Write;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::Result;

pub fn configure(cfg: &mut ServiceConfig) {
    cfg
        .service(web::resource("/books")
            .route(web::get().to(list_books))
            .route(web::post()
                .guard(ContentType::from_mime(&mime::APPLICATION_JSON))
                .to(create_book))
            .route(web::post().to_async(create_book_from_zip))
        )
        .service(web::resource("/books/{id}")
            .route(web::get().to(get_book))
            .route(web::put()
                .guard(ContentType::from_mime(&mime::APPLICATION_JSON))
                .to(update_book))
            .route(web::put().to_async(replace_book))
            .route(web::delete().to(delete_book))
        )
        .service(web::resource("/books/{id}/parts")
            .route(web::get().to(book_contents))
            .route(web::post().to(create_part))
        )
        .service(web::resource("/books/{id}/parts/{number}")
            .route(web::get().to(get_part))
            .route(web::delete().to(delete_part))
            .route(web::put().to(update_part))
        )
    ;
}

/// List all books.
///
/// ## Method
///
/// ```text
/// GET /books
/// ```
fn list_books(db: Database, session: Session)
-> Result<Json<Vec<<Book as Model>::Public>>> {
    let books = if session.permissions().contains(SystemPermissions::MANAGE_TEAM) {
        Book::all(&db)?
    } else {
        let user = session.user(&db)?;
        let teams = user.get_team_ids(&db)?;

        Book::by_team(&db, &teams)?
    };

    Ok(Json(books.get_public()))
}

#[derive(Deserialize)]
struct NewBook {
    title: String,
    team: i32,
}

/// Create a new book.
///
/// ## Method
///
/// ```text
/// POST /books
/// Content-Type: application/json
/// ```
fn create_book(
    req: HttpRequest,
    db: Database,
    session: Session,
    form: Json<NewBook>,
) -> Result<Created<String, Json<<Book as Model>::Public>>> {
    let team = Team::by_id(&db, form.team)?;

    if !session.is_elevated {
        team.get_member(&db, &session.user(&db)?)
            .map_err(|err| match err {
                FindModelError::NotFound(_) => FindModelError::<Book>::not_found(),
                FindModelError::Database(_, err) =>
                    FindModelError::<Book>::from(err),
            })?
            .permissions()
            .require(TeamPermissions::EDIT_BOOK)?;
    }

    let book = Book::create(&db, &team, &form.title)?;
    let location = format!(
        "{}/api/v1/books/{}", req.app_config().host(), book.id());
    Ok(Created(location, Json(book.get_public())))
}

#[derive(FromMultipart)]
struct NewBookZip {
    title: String,
    team: FromStrField<i32>,
    file: NamedTempFile,
}

/// Create a new book, populating it with contents of a ZIP archive.
///
/// ## Method
///
/// ```text
/// POST /books
/// Content-Type: multipart/form-data
/// ```
fn create_book_from_zip(
    req: HttpRequest,
    db: Database,
    importer: Data<Addr<Importer>>,
    session: Session,
    data: Multipart<NewBookZip>,
) -> Box<dyn Future<Item = Created<String, Json<<Book as Model>::Public>>, Error = Error>> {
    let NewBookZip { title, team, file } = data.into_inner();

    let team = match Team::by_id(&db, *team) {
        Ok(team) => team,
        Err(err) => return Box::new(future::err(err.into())),
    };

    match session.user(&db)
        .map_err(Error::from)
        .and_then(|user| {
            if session.is_elevated {
                Ok(TeamPermissions::all())
            } else {
                team.get_member(&db, &user)
                    .map(|tm| tm.permissions())
                    .map_err(|err| match err {
                        FindModelError::NotFound(_) =>
                            FindModelError::<Book>::not_found(),
                        FindModelError::Database(_, err) =>
                            FindModelError::<Book>::from(err),
                    })
                    .map_err(Error::from)
            }
        })
        .and_then(|permissions| {
            permissions
                .require(TeamPermissions::EDIT_BOOK)
                .map_err(Error::from)
        })
    {
        Ok(()) => (),
        Err(err) => return Box::new(future::err(err)),
    }

    Box::new(importer.send(ImportBook {
        title, team, file,
        actor: session.user_id().into(),
    })
        .from_err::<Error>()
        .and_then(|r| future::result(r).from_err())
        .map(move |book| {
            let location = format!("{}/api/v1/books/{}",
                req.app_config().host(), book.id);
            Created(location, Json(book.get_public()))
        }))
}

/// Get a book by ID.
///
/// ## Method
///
/// ```text
/// GET /books/:id
/// ```
fn get_book(scope: TeamScoped<Book>) -> Result<Json<<Book as Model>::Public>> {
    Ok(Json(scope.resource().get_public()))
}

#[derive(Deserialize)]
struct BookChange {
    title: String,
}

/// Update books metadata.
///
/// ## Method
///
/// ```text
/// PUT /books/:id
/// Content-Type: application/json
/// ```
fn update_book(
    db: Database,
    scope: TeamScoped<Book, EditBook>,
    change: Json<BookChange>,
) -> Result<Json<<Book as Model>::Public>> {
    let mut book = scope.into_resource();

    book.set_title(&db, change.into_inner().title)?;

    Ok(Json(book.get_public()))
}

/// Replace contents of a book.
///
/// ## Method
///
/// ```text
/// PUT /books/:id
/// ```
fn replace_book(
    db: Database,
    importer: Data<Addr<Importer>>,
    scope: TeamScoped<Book, EditBook>,
    session: Session,
    payload: Payload,
) -> Box<dyn Future<Item = Json<<Book as Model>::Public>, Error = Error>> {
    let book = scope.into_resource();
    let file = match NamedTempFile::new() {
        Ok(file) => file,
        Err(err) => return Box::new(future::err(err.into())),
    };
    let team = match book.get_team(&db) {
        Ok(team) => team,
        Err(err) => return Box::new(future::err(err.into())),
    };

    Box::new(payload
        .from_err::<Error>()
        .fold(file, |mut file, chunk| match file.write_all(chunk.as_ref()) {
            Ok(_) => future::ok(file),
            Err(err) => future::err(err),
        })
        .and_then(move |file| importer.send(ReplaceBook {
            team, book, file,
            actor: session.user_id().into(),
        }).from_err())
        .and_then(|r| future::result(r).from_err())
        .map(|book: Book| book.get_public())
        .map(Json))
}

/// Delete a book by ID.
///
/// ## Method
///
/// ```text
/// DELETE /books/:id
/// ```
fn delete_book(db: Database, scope: TeamScoped<Book, EditBook>)
-> Result<HttpResponse> {
    scope.into_resource().delete(&db)?;
    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

/// Get book's contents as a tree.
///
/// ## Method
///
/// ```text
/// GET /books/:id/parts
/// ```
fn book_contents(db: Database, scope: TeamScoped<Book>)
-> Result<Json<Tree>> {
    Ok(Json(scope.resource().root_part(&db)?.get_tree(&db)?))
}

#[derive(Deserialize)]
struct NewTreeRoot {
    #[serde(flatten)]
    tree: NewTree,
    parent: i32,
    index: i32,
}

/// Create a new part.
///
/// ## Method
///
/// ```text
/// POST /books/:id/parts
/// ```
fn create_part(
    req: HttpRequest,
    db: Database,
    scope: TeamScoped<Book, EditBook>,
    tree: Json<NewTreeRoot>,
) -> Result<Created<String, Json<Tree>>> {
    let NewTreeRoot { tree, parent, index } = tree.into_inner();
    let book = scope.resource();
    let parent = book.get_part(&db, parent)?;
    let tree = parent.create_tree(&db, index, tree)?;
    let location = format!("{}/api/v1/books/{}/parts/{}",
        req.app_config().host(), book.id(), tree.number);

    Ok(Created(location, Json(tree)))
}

/// Inspect a single part of a book.
///
/// ## Method
///
/// ```text
/// GET /books/:id/parts/:number
/// ```
fn get_part(db: Database, scope: TeamScoped<Book>, id: Path<(Uuid, i32)>)
-> Result<Json<<BookPart as Model>::Public>> {
    let (_, part_id) = id.into_inner();
    Ok(Json(scope.resource().get_part(&db, part_id)?.get_public()))
}

/// Delete a part from a book.
///
/// ## Method
///
/// ```text
/// DELETE /book/:id/parts/:number
/// ```
fn delete_part(
    db: Database,
    scope: TeamScoped<Book, EditBook>,
    id: Path<(Uuid, i32)>,
) -> Result<HttpResponse> {
    let (_, part_id) = id.into_inner();

    scope.resource().get_part(&db, part_id)?.delete(&db)?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

#[derive(Deserialize)]
struct PartUpdate {
    title: Option<String>,
    #[serde(flatten)]
    location: Option<PartLocation>,
}

#[derive(Deserialize)]
struct PartLocation {
    parent: i32,
    index: i32,
}

/// Update a book part.
///
/// ## Method
///
/// ```text
/// PUT /books/:id/parts/:number
/// ```
fn update_part(
    db: Database,
    scope: TeamScoped<Book, EditBook>,
    id: Path<(Uuid, i32)>,
    update: Json<PartUpdate>,
) -> Result<HttpResponse> {
    let PartUpdate { title, location } = update.into_inner();
    let (_, part_id) = id.into_inner();

    let book = scope.into_resource();
    let mut part = book.get_part(&db, part_id)?;

    let parent = location.as_ref()
        .map_or(Ok(None), |location|
            book.get_part(&db, location.parent)
                .map(|part| Some((part, location.index)))
        )?;

    let db = &db;
    db.transaction::<_, Error, _>(move || {
        if let Some(ref title) = title {
            part.set_title(&db, &title)?;
        }

        if let Some((parent, index)) = parent {
            part.reparent(&db, &parent, index)?;
        }

        Ok(())
    })?;

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}
