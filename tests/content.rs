//! Tests for content management (books, modules, drafts, files, etc.).

use actix_web::http::StatusCode;
use adaptarr::{
    db::{
        models as db,
        schema::{books, documents, modules, document_files, xref_targets},
        types::SlotPermission,
    },
    models::{
        Book,
        User,
        Module,
        File,
        bookpart::NewTree,
        editing::{Process, structure},
    },
    permissions::PermissionBits,
};
use diesel::prelude::*;
use failure::Fallible;
use uuid::Uuid;
use lazy_static::lazy_static;

mod common;

use self::common::{Client, Connection, models::*};

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

    User::create(
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
            },
            db::Module {
                id: *M2,
                document: d2.id,
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

    let process = Process::create(&*db, &structure::Process {
        name: "Test process".into(),
        start: 0,
        slots: vec![
            structure::Slot {
                id: 0,
                name: "Slot".into(),
                role: None,
                autofill: false,
            },
            structure::Slot {
                id: 1,
                name: "Another slot".into(),
                role: None,
                autofill: false,
            },
        ],
        steps: vec![
            structure::Step {
                id: 0,
                name: "Start".into(),
                slots: vec![
                    structure::StepSlot {
                        slot: 0,
                        permission: SlotPermission::Edit,
                    },
                    structure::StepSlot {
                        slot: 1,
                        permission: SlotPermission::View,
                    },
                ],
                links: vec![
                    structure::Link {
                        name: "Link".into(),
                        to: 1,
                        slot: 0,
                    },
                ],
            },
            structure::Step {
                id: 0,
                name: "End".into(),
                slots: vec![],
                links: vec![],
            },
        ],
    })?;

    let slot = process.get_slot(&*db, 1)?;

    m2.begin_process(&*db, &process, std::iter::once((slot, user)))?;

    Ok(())
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
            document: DocumentData {
                title: "First test".into(),
                language: "en".into(),
            },
        },
        ModuleData {
            id: *M2,
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
        document: DocumentData {
            title: "First test".into(),
            language: "en".into(),
        },
    });
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
            permissions: vec![SlotPermission::Edit],
            step: Some(StepData {
                id: 1,
                process: [1, 1],
                name: "Start".into(),
                links: vec![
                    LinkData {
                        name: "Link".into(),
                        to: 2,
                        slot: 1,
                    },
                ],
            }),
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
        permissions: vec![SlotPermission::Edit],
        step: Some(StepData {
            id: 1,
            process: [1, 1],
            name: "Start".into(),
            links: vec![
                LinkData {
                    name: "Link".into(),
                    to: 2,
                    slot: 1,
                },
            ],
        }),
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
    let data = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/books")
        .send()
        .assert_success()
        .json::<Vec<Uuid>>();

    assert_eq!(data, [*B1]);
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn api_details_of_drafts_process(mut client: Client) {
    let mut data = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/process")
        .send()
        .assert_success()
        .json::<ProcessDetails>();

    data.slots.sort_by_key(|ss| ss.slot.id);

    assert_eq!(data, ProcessDetails {
        process: VersionData {
            id: 1,
            name: "Test process".into(),
            version: data.process.version,
        },
        slots: vec![
            SlotSeating {
                slot: SlotData {
                    id: 1,
                    name: "Slot".into(),
                    role: None,
                },
                user: Some(UserData {
                    id: 1,
                    name: "User".into(),
                    is_super: false,
                    language: "en".into(),
                    permissions: None,
                    role: None,
                }),
            },
            SlotSeating {
                slot: SlotData {
                    id: 2,
                    name: "Another slot".into(),
                    role: None,
                },
                user: None,
            },
        ],
    });
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn api_assign_user_to_slot_in_draft(mut client: Client) {
    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/process/slots/1")
        .json(2)
        .assert_success();

    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/process/slots/2")
        .json(1)
        .assert_success();

    let mut data = client.get("/api/v1/drafts/22222222-2222-2222-2222-222222222222/process")
        .send()
        .assert_success()
        .json::<ProcessDetails>();

    data.slots.sort_by_key(|ss| ss.slot.id);

    assert_eq!(data, ProcessDetails {
        process: VersionData {
            id: 1,
            name: "Test process".into(),
            version: data.process.version,
        },
        slots: vec![
            SlotSeating {
                slot: SlotData {
                    id: 1,
                    name: "Slot".into(),
                    role: None,
                },
                user: Some(UserData {
                    id: 2,
                    name: "Administrator".into(),
                    is_super: true,
                    language: "en".into(),
                    permissions: None,
                    role: None,
                }),
            },
            SlotSeating {
                slot: SlotData {
                    id: 2,
                    name: "Another slot".into(),
                    role: None,
                },
                user: Some(UserData {
                    id: 1,
                    name: "User".into(),
                    is_super: false,
                    language: "en".into(),
                    permissions: None,
                    role: None,
                }),
            },
        ],
    });
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

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn create_draft_of_module(mut client: Client) {
    let data = client.post("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .json(BeginProcess {
            process: 1,
            slots: vec![(1, 2)],
        })
        .assert_success()
        .json::<DraftData>();

    assert_eq!(data, DraftData {
        module: *M1,
        document: DocumentData {
            title: "First test".into(),
            language: "en".into(),
        },
        permissions: vec![SlotPermission::Edit],
        step: Some(StepData {
            id: 1,
            process: [1, 1],
            name: "Start".into(),
            links: vec![
                LinkData {
                    name: "Link".into(),
                    to: 2,
                    slot: 1,
                },
            ],
        }),
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn cannot_create_draft_of_module_without_permissions(mut client: Client) {
    client.post("/api/v1/modules/11111111-1111-1111-1111-111111111111")
        .json(BeginProcess {
            process: 1,
            slots: vec![],
        })
        .assert_error(StatusCode::BAD_REQUEST, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test", elevated = true))]
fn cannot_create_second_draft_of_module(mut client: Client) {
    client.post("/api/v1/modules/22222222-2222-2222-2222-222222222222")
        .json(BeginProcess {
            process: 1,
            slots: vec![],
        })
        .assert_error(StatusCode::BAD_REQUEST, "draft:create:exists");
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
        permissions: vec![SlotPermission::Edit],
        step: Some(StepData {
            id: 1,
            process: [1, 1],
            name: "Start".into(),
            links: vec![
                LinkData {
                    name: "Link".into(),
                    to: 2,
                    slot: 1,
                },
            ],
        }),
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn save_draft(mut client: Client) {
    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/index.cnxml")
        .body(b"Changed".as_ref())
        .assert_success();

    client.put("/api/v1/drafts/22222222-2222-2222-2222-222222222222/files/new-file")
        .body(b"New".as_ref())
        .assert_success();

    client.post("/api/v1/drafts/22222222-2222-2222-2222-222222222222/advance")
        .json(AdvanceDraft {
            target: 2,
            slot: 1,
        })
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

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_free_slots(mut client: Client) {
    let data = client.get("/api/v1/processes/slots/free")
        .send()
        .assert_success()
        .json::<Vec<FreeSlot>>();

    assert_eq!(data, [
        FreeSlot {
            slot: SlotData {
                id: 2,
                name: "Another slot".into(),
                role: None,
            },
            draft: DraftData {
                module: *M2,
                document: DocumentData {
                    title: "Second test".into(),
                    language: "pl".into(),
                },
                permissions: Vec::new(),
                step: None,
            },
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_take_free_slot(mut client: Client) {
    client.post("/api/v1/processes/slots")
        .json(AssignToSlot {
            draft: *M2,
            slot: 2,
        })
        .assert_success();

    let data = client.get("/api/v1/processes/slots/free")
        .send()
        .assert_success()
        .json::<Vec<FreeSlot>>();

    assert_eq!(data, []);
}
