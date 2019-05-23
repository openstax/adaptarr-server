//! Tests for content management (books, modules, drafts, files, etc.).

// #[macro_use] extern crate lazy_static;

use actix_web::http::StatusCode;
use adaptarr::{
    db::{
        models as db,
        schema::{books, documents, modules, document_files, xref_targets},
    },
    models::{Book, User, Module, File, bookpart::NewTree},
    permissions::PermissionBits,
};
use diesel::prelude::*;
use failure::Fallible;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use uuid::Uuid;
use lazy_static::lazy_static;

mod common;

use self::common::{Client, Connection, Pooled};

lazy_static! {
    static ref M1: Uuid = Uuid::from_bytes([0x11; 16]);

    static ref M2: Uuid = Uuid::from_bytes([0x22; 16]);

    static ref B1: Uuid = Uuid::from_bytes([0xaa; 16]);
}

#[adaptarr::test_database]
fn setup_db(db: &Connection) -> Result<(), failure::Error> {
    let user = User::create(
        db,
        "user@adaptarr.test",
        "User",
        "test",
        false,
        "en",
        PermissionBits::empty(),
    )?;

    let admin = User::create(
        db,
        "administrator@adaptarr.test",
        "Administrator",
        "test",
        true,
        "en",
        PermissionBits::all(),
    )?;

    let i1 = File::from_data(db, &common::CONFIG, b"First test index.cnxml")?;
    let i2 = File::from_data(db, &common::CONFIG, b"Second test index.cnxml")?;
    let f1 = File::from_data(db, &common::CONFIG, b"Test file")?;

    let mut documents = diesel::insert_into(documents::table)
        .values([
            db::NewDocument {
                title: "First test",
                language: "en",
                index: i1.id,
            },
            db::NewDocument {
                title: "Second test",
                language: "pl",
                index: i2.id,
            },
        ].as_ref())
        .get_results::<db::Document>(db)?
        .into_iter();
    let d1 = documents.next().unwrap();
    let d2 = documents.next().unwrap();

    diesel::insert_into(modules::table)
        .values([
            db::Module {
                id: *M1,
                document: d1.id,
                assignee: Some(admin.id),
            },
            db::Module {
                id: *M2,
                document: d2.id,
                assignee: Some(user.id),
            },
        ].as_ref())
        .execute(db)?;

    diesel::insert_into(document_files::table)
        .values(&db::NewDocumentFile {
            document: d2.id,
            name: "file",
            file: f1.id,
        })
        .execute(db)?;

    diesel::insert_into(xref_targets::table)
        .values([
            db::NewXrefTarget {
                document: d1.id,
                element: "test1",
                type_: "TYPE1",
                description: None,
                context: None,
                counter: 1,
            },
            db::NewXrefTarget {
                document: d1.id,
                element: "test2",
                type_: "TYPE2",
                description: Some("A description"),
                context: Some("test1"),
                counter: 2,
            },
        ].as_ref())
        .execute(db)?;

    diesel::update(&d1)
        .set(documents::xrefs_ready.eq(true))
        .execute(db)?;

    let m1 = Module::by_id(db, *M1)?;
    let m2 = Module::by_id(db, *M2)?;

    diesel::insert_into(books::table)
        .values(&db::NewBook {
            id: *B1,
            title: "Test book",
        })
        .execute(db)?;

    let book = Book::by_id(db, *B1)?.root_part(db)?;
    book.insert_module(db, 0, "Introduction", &m1)?;
    book.create_tree(db, 1, NewTree::Group {
        title: "Group".to_string(),
        parts: vec![
            NewTree::Module {
                title: None,
                module: m2.id(),
            },
        ],
    })?;

    m2.create_draft(db, user.id)?;

    Ok(())
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct DocumentData<'a> {
    title: Cow<'a, str>,
    language: Cow<'a, str>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct ModuleData<'a> {
    id: Uuid,
    assignee: Option<i32>,
    #[serde(flatten)]
    document: DocumentData<'a>,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_modules(mut client: Client) {
    let data = client.get("/api/v1/modules")
        .send()
        .assert_success()
        .json::<Vec<ModuleData>>();

    assert_eq!(data, [
        ModuleData {
            id: *M1,
            assignee: Some(2),
            document: DocumentData {
                title: "First test".into(),
                language: "en".into(),
            },
        },
        ModuleData {
            id: *M2,
            assignee: Some(1),
            document: DocumentData {
                title: "Second test".into(),
                language: "pl".into(),
            },
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_modules_assigned_to_user(mut client: Client) {
    let data = client.get("/api/v1/modules/assigned/to/1")
        .send()
        .assert_success()
        .json::<Vec<ModuleData>>();

    assert_eq!(data, [
        ModuleData {
            id: *M2,
            assignee: Some(1),
            document: DocumentData {
                title: "Second test".into(),
                language: "pl".into(),
            },
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_module(mut client: Client) {
    let data = client.get("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .send()
        .assert_success()
        .json::<ModuleData>();

    assert_eq!(data, ModuleData {
        id: *M1,
        assignee: Some(2),
        document: DocumentData {
            title: "First test".into(),
            language: "en".into(),
        },
    });
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct FileInfo<'a> {
    name: Cow<'a, str>,
    mime: Cow<'a, str>,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_list_of_modules_files(mut client: Client) {
    let data = client.get("api/v1/modules/22222222-2222-2222-2222-222222222222/files")
        .send()
        .assert_success()
        .json::<Vec<FileInfo>>();

    assert_eq!(data, [
        FileInfo {
            name: "file".into(),
            mime: "text/plain".into(),
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_module_file(mut client: Client) {
    let data = client.get("api/v1/modules/22222222-2222-2222-2222-222222222222/files/file")
        .send()
        .assert_success()
        .body();

    assert_eq!(data, b"Test file".as_ref());
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct XrefData<'a> {
    id: Cow<'a, str>,
    #[serde(rename = "type")]
    type_: Cow<'a, str>,
    description: Option<Cow<'a, str>>,
    context: Option<Cow<'a, str>>,
    counter: i32,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_list_of_reference_targets_in_module(mut client: Client) {
    let data = client.get("/api/v1/modules/11111111-1111-1111-1111-111111111111/xref-targets")
        .send()
        .assert_success()
        .json::<Vec<XrefData>>();

    assert_eq!(data, [
        XrefData {
            id: "test1".into(),
            type_: "TYPE1".into(),
            description: None,
            context: None,
            counter: 1,
        },
        XrefData {
            id: "test2".into(),
            type_: "TYPE2".into(),
            description: Some("A description".into()),
            context: Some("test1".into()),
            counter: 2,
        },
    ]);
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn list_of_references_not_ready_for_just_created_module(mut client: Client) {
    let data = client.post("/api/v1/modules")
        .json(NewModule {
            title: "New module",
            language: "en",
        })
        .assert_success()
        .json::<ModuleData>();

    client.get(&format!("/api/v1/modules/{}/xref-targets", data.id))
        .send()
        .assert_error(StatusCode::SERVICE_UNAVAILABLE, "module:xref:not-ready");
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_list_of_books_containing_module(mut client: Client) {
    let data = client.get("/api/v1/modules/11111111-1111-1111-1111-111111111111/books")
        .send()
        .assert_success()
        .json::<Vec<Uuid>>();

    assert_eq!(data, [*B1]);
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct BookData<'a> {
    id: Uuid,
    title: Cow<'a, str>,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_books(mut client: Client) {
    let data = client.get("/api/v1/books")
        .send()
        .assert_success()
        .json::<Vec<BookData>>();

    assert_eq!(data, [
        BookData {
            id: *B1,
            title: "Test book".into(),
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_book(mut client: Client) {
    let data = client.get("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        .send()
        .assert_success()
        .json::<BookData>();

    assert_eq!(data, BookData {
        id: *B1,
        title: "Test book".into(),
    });
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct Tree<'a> {
    number: i32,
    title: Cow<'a, str>,
    #[serde(flatten)]
    part: Variant<Tree<'a>>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Variant<Part> {
    Module {
        id: Uuid,
    },
    Group {
        parts: Vec<Part>,
    },
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_books_parts(mut client: Client) {
    let data = client.get("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/parts")
        .send()
        .assert_success()
        .json::<Tree>();

    assert_eq!(data, Tree {
        number: 0,
        title: "Test book".into(),
        part: Variant::Group {
            parts: vec![
                Tree {
                    number: 1,
                    title: "Introduction".into(),
                    part: Variant::Module {
                        id: *M1,
                    },
                },
                Tree {
                    number: 2,
                    title: "Group".into(),
                    part: Variant::Group {
                        parts: vec![
                            Tree {
                                number: 3,
                                title: "Second test".into(),
                                part: Variant::Module {
                                    id: *M2,
                                },
                            },
                        ],
                    },
                },
            ],
        },
    });
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct PartData<'a> {
    number: i32,
    title: Cow<'a, str>,
    #[serde(flatten)]
    part: Variant<i32>,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_book_part(mut client: Client) {
    let data = client.get("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/parts/2")
        .send()
        .assert_success()
        .json::<PartData>();

    assert_eq!(data, PartData {
        number: 2,
        title: "Group".into(),
        part: Variant::Group {
            parts: vec![3],
        },
    });
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct DraftData<'a> {
    module: Uuid,
    #[serde(flatten)]
    document: DocumentData<'a>,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_drafts(mut client: Client) {
    let data = client.get("/api/v1/drafts")
        .send()
        .assert_success()
        .json::<Vec<DraftData>>();

    assert_eq!(data, [
        DraftData {
            module: *M2,
            document: DocumentData {
                title: "Second test".into(),
                language: "pl".into(),
            },
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_draft(mut client: Client) {
    let data = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222")
        .send()
        .assert_success()
        .json::<DraftData>();

    assert_eq!(data, DraftData {
        module: *M2,
        document: DocumentData {
            title: "Second test".into(),
            language: "pl".into(),
        },
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_drafts_files(mut client: Client) {
    let data = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files")
        .send()
        .assert_success()
        .json::<Vec<FileInfo>>();

    assert_eq!(data, [
        FileInfo {
            name: "file".into(),
            mime: "text/plain".into(),
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_draft_file(mut client: Client) {
    let data = client.get("api/v1/drafts/22222222-2222-2222-2222-222222222222/files/file")
        .send()
        .assert_success()
        .body();

    assert_eq!(data, b"Test file".as_ref());
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_books_containing_draft(mut client: Client) {
    let data = client.get("/api/v1/modules/11111111-1111-1111-1111-111111111111/books")
        .send()
        .assert_success()
        .json::<Vec<Uuid>>();

    assert_eq!(data, [*B1]);
}

#[derive(Debug, Serialize)]
struct NewModule<'a> {
    title: &'a str,
    language: &'a str,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn creating_module_requires_permission(mut client: Client) {
    client.post("/api/v1/modules")
        .json(NewModule {
            title: "New module",
            language: "en",
        })
    .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn create_empty_module(mut client: Client) {
    let data = client.post("/api/v1/modules")
        .json(NewModule {
            title: "New module",
            language: "en",
        })
        .assert_success()
        .json::<ModuleData>();

    assert_eq!(data, ModuleData {
        id: data.id,
        assignee: None,
        document: DocumentData {
            title: "New module".into(),
            language: "en".into(),
        },
    });
}

#[adaptarr::test]
#[ignore]
fn create_module_from_zip() {
    // TODO: write test
}

#[adaptarr::test]
#[ignore]
fn creating_module_from_zip_requires_permission() {
    // TODO: write test
}

#[derive(Debug, Serialize)]
pub struct ModuleUpdate {
    pub assignee: Option<i32>,
}

// TODO: Module deletion is not yet implemented.
#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
#[ignore]
fn delete_module(mut client: Client) {
    client.delete("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .send()
        .assert_success();

    client.get("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .send()
        .assert_error(StatusCode::NOT_FOUND, "module:not-found");
}

// TODO: Module deletion is not yet implemented.
#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
#[ignore]
fn deleting_module_requires_permission(mut client: Client) {
    client.delete("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .send()
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn assign_user_to_module(db: Pooled, mut client: Client) -> Fallible<()> {
    client.put("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .json(ModuleUpdate {
            assignee: Some(1234567890),
        })
        .assert_error(StatusCode::BAD_REQUEST, "user:not-found");

    let user = User::by_email(&*db, "user@adaptarr.test")?;

    client.put("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .json(ModuleUpdate {
            assignee: Some(user.id),
        })
        .assert_success();

    let module = Module::by_id(&*db, *M1)?.into_db().0;

    assert_eq!(module.assignee, Some(user.id));

    client.put("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .json(ModuleUpdate {
            assignee: None,
        })
        .assert_success();

    let module = Module::by_id(&*db, *M1)?.into_db().0;

    assert_eq!(module.assignee, None);

    Ok(())
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn assigning_user_to_module_requires_permission(mut client: Client) {
    client.put("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .json(ModuleUpdate {
            assignee: Some(1234567890),
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[derive(Debug, Serialize)]
struct NewBook<'a> {
    title: &'a str,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn creating_book_requires_permission(mut client: Client) {
    client.post("/api/v1/books")
        .json(NewBook {
            title: "New book",
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
#[ignore]
fn creating_book_from_zip_requires_permission() -> Fallible<()> {
    // TODO: implement
    unimplemented!()
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn create_empty_book(mut client: Client) {
    let data = client.post("/api/v1/books")
        .json(NewBook {
            title: "New book",
        })
        .assert_success()
        .json::<BookData>();

    assert_eq!(data.title, "New book");
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
#[ignore]
fn create_book_from_zip() -> Fallible<()> {
    // TODO: implement
    unimplemented!()
}

#[derive(Debug, Serialize)]
struct BookChange<'a> {
    title: &'a str,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn updating_book_metadata_requires_permission(mut client: Client) {
    client.put("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        .json(BookChange {
            title: "New title",
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn update_book_metadata(mut client: Client) {
    let data = client.put("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        .json(BookChange {
            title: "New title",
        })
        .assert_success()
        .json::<BookData>();

    assert_eq!(data, BookData {
        id: *B1,
        title: "New title".into(),
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
#[ignore]
fn replacing_book_contents_requires_permission() -> Fallible<()> {
    // TODO: implement
    unimplemented!()
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
#[ignore]
fn replace_book_contents() -> Fallible<()> {
    // TODO: implement
    unimplemented!()
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn deleting_book_requires_permission(mut client: Client) {
    client.delete("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        .send()
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn delete_book(mut client: Client) {
    client.delete("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        .send()
        .assert_success();

    client.get("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        .send()
        .assert_error(StatusCode::NOT_FOUND, "book:not-found");
}

#[derive(Debug, Serialize)]
struct NewTreeRoot {
    #[serde(flatten)]
    tree: NewTree,
    parent: i32,
    index: i32,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn editing_book_structure_requires_permission(mut client: Client) {
    client.post("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/parts")
        .json(NewTreeRoot {
            tree: NewTree::Module {
                title: None,
                module: *M1,
            },
            parent: 0,
            index: 0,
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn create_book_part(mut client: Client) {
    let data = client.post("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/parts")
        .json(NewTreeRoot {
            tree: NewTree::Module {
                title: None,
                module: *M1,
            },
            parent: 0,
            index: 0,
        })
        .assert_success()
        .json::<Tree>();

    assert_eq!(data, Tree {
        number: data.number,
        title: "First test".into(),
        part: Variant::Module {
            id: *M1,
        },
    });
}

#[derive(Debug, Serialize)]
struct PartUpdate<'a> {
    title: &'a str,
    #[serde(flatten)]
    location: PartLocation,
}

#[derive(Debug, Serialize)]
struct PartLocation {
    parent: i32,
    index: i32,
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true, permissions = PermissionBits::EDIT_BOOK))]
fn update_book_part(mut client: Client) {
    client.put("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/parts/1")
        .json(PartUpdate {
            title: "New name",
            location: PartLocation {
                parent: 2,
                index: 0,
            },
        })
        .assert_success();

    let data = client.get("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/parts")
        .send()
        .assert_success()
        .json::<Tree>();

    assert_eq!(data, Tree {
        number: 0,
        title: "Test book".into(),
        part: Variant::Group {
            parts: vec![
                Tree {
                    number: 2,
                    title: "Group".into(),
                    part: Variant::Group {
                        parts: vec![
                            Tree {
                                number: 1,
                                title: "New name".into(),
                                part: Variant::Module {
                                    id: *M1,
                                },
                            },
                            Tree {
                                number: 3,
                                title: "Second test".into(),
                                part: Variant::Module {
                                    id: *M2,
                                },
                            },
                        ],
                    },
                },
            ],
        },
    });
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn delete_book_part(mut client: Client) {
    client.delete("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/parts/2")
        .send()
        .assert_success();

    let data = client.get("/api/v1/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/parts")
        .send()
        .assert_success()
        .json::<Tree>();

    assert_eq!(data, Tree {
        number: 0,
        title: "Test book".into(),
        part: Variant::Group {
            parts: vec![
                Tree {
                    number: 1,
                    title: "Introduction".into(),
                    part: Variant::Module {
                        id: *M1,
                    },
                },
            ],
        },
    });
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test"))]
fn create_draft_of_module(mut client: Client) {
    let data = client.post("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .send()
        .assert_success()
        .json::<DraftData>();

    assert_eq!(data, DraftData {
        module: *M1,
        document: DocumentData {
            title: "First test".into(),
            language: "en".into(),
        },
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn cannot_create_draft_of_module_if_not_assigned(mut client: Client) {
    client.post("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .send()
        .assert_error(StatusCode::BAD_REQUEST, "draft:create:not-assigned");
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn cannot_create_second_draft_of_module(mut client: Client) {
    client.post("/api/v1/modules/22222222-2222-2222-2222-222222222222")
        .send()
        .assert_error(StatusCode::BAD_REQUEST, "draft:create:exists");
}

#[derive(Debug, Serialize)]
struct DraftUpdate<'a> {
    title: &'a str,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test", elevated = true))]
fn update_draft_metadata(mut client: Client) {
    let data = client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222")
        .json(DraftUpdate {
            title: "New title",
        })
        .assert_success()
        .json::<DraftData>();

    assert_eq!(data, DraftData {
        module: *M2,
        document: DocumentData {
            title: "New title".into(),
            language: "pl".into(),
        },
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn delete_draft(mut client: Client) {
    client.delete("/api/v1/drafts/22222222-2222-2222-2222-222222222222")
        .send()
        .assert_success();

    client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222")
        .send()
        .assert_error(StatusCode::NOT_FOUND, "draft:not-found");
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn save_draft(mut client: Client) {
    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/index.cnxml")
        .body(b"Changed".as_ref())
        .assert_success();

    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .body(b"New".as_ref())
        .assert_success();

    client.post("/api/v1/drafts/22222222-2222-2222-2222-222222222222/save")
        .send()
        .assert_success();

    let changed = client.get("/api/v1/modules/22222222-2222-2222-2222-222222222222/files/index.cnxml")
        .send()
        .assert_success()
        .body();

    assert_eq!(changed, b"Changed".as_ref());

    let new = client.get("/api/v1/modules/22222222-2222-2222-2222-222222222222/files/new-file")
        .send()
        .assert_success()
        .body();

    assert_eq!(new, b"New".as_ref());
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn add_file_to_draft(mut client: Client) {
    client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .send()
        .assert_error(StatusCode::NOT_FOUND, "file:not-found");

    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .body(b"New".as_ref())
        .assert_success();

    let after = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .send()
        .body();

    assert_eq!(after, b"New".as_ref());
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn update_file_in_draft(mut client: Client) {
    let before = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/index.cnxml")
        .send()
        .body();

    assert_eq!(before, b"Second test index.cnxml".as_ref());

    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/index.cnxml")
        .body(b"Changed".as_ref())
        .assert_success();

    let after = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/index.cnxml")
        .send()
        .body();

    assert_eq!(after, b"Changed".as_ref());
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn delete_file_in_draft(mut client: Client) {
    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .body(b"New".as_ref())
        .assert_success();

    let after = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .send()
        .body();

    assert_eq!(after, b"New".as_ref());

    client.delete("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .send()
        .assert_success();

    client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .send()
        .assert_error(StatusCode::NOT_FOUND, "file:not-found");
}
