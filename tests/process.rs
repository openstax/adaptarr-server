//! Test for managing editing processes.
//!
//! This suite only contains tests for editing processes. Tests for interactions
//! between processes, drafts, and such are included in the content suite.

// NOTE: Since the database is cleared before each test, the _Old editing
// process_ will never be returned.

use actix_web::http::StatusCode;
use adaptarr::{
    db::types::SlotPermission,
    models::{User, editing::{Process, structure}},
    permissions::PermissionBits,
};
use failure::Fallible;
use lazy_static::lazy_static;

mod common;

use self::common::{Connection, Client, models::*};

lazy_static! {
    static ref TEST_PROCESS: structure::Process = structure::Process {
        name: "Test process".into(),
        start: 0,
        slots: vec![
            structure::Slot {
                id: 0,
                name: "Slot".into(),
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
    };

    static ref TEST_PROCESS_DB: structure::Process = {
        let mut p = TEST_PROCESS.clone();
        p.slots[0].id = 1;
        p.steps[0].id = 1;
        p.steps[1].id = 2;
        p
    };
}

#[adaptarr::test_database]
fn setup_db(db: &Connection) -> Fallible<()> {
    User::create(
        db,
        "user@adaptarr.test",
        "User",
        "test",
        false,
        "en",
        PermissionBits::EDIT_PROCESS | PermissionBits::MANAGE_PROCESS,
    )?;

    Process::create(&*db, &TEST_PROCESS)?;

    Ok(())
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_processes(mut client: Client) {
    let data = client.get("/api/v1/processes")
        .send()
        .assert_success()
        .json::<Vec<ProcessData>>();

    assert_eq!(data, [
        ProcessData {
            id: 1,
            name: "Test process".into(),
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_process(mut client: Client) {
    let data = client.get("/api/v1/processes/1")
        .send()
        .assert_success()
        .json::<ProcessData>();

    assert_eq!(data, ProcessData {
        id: 1,
        name: "Test process".into(),
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_process_structure(mut client: Client) {
    let data = client.get("/api/v1/processes/1/structure")
        .send()
        .assert_success()
        .json::<structure::Process>();

    assert_eq!(data, *TEST_PROCESS_DB);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test", elevated = true))]
fn api_create_process(mut client: Client) {
    let mut input = TEST_PROCESS.clone();
    input.name = "Another edit process".into();

    let data = client.post("/api/v1/processes")
        .json(input)
        .assert_success()
        .json::<ProcessData>();

    assert_eq!(data, ProcessData {
        id: 2,
        name: "Another edit process".into(),
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_creating_process_requires_permissions(mut client: Client) {
    client.post("/api/v1/processes")
        .json(&*TEST_PROCESS)
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "user@adaptarr.test", elevated = true))]
fn api_update_process(mut client: Client) {
    let data = client.put("/api/v1/processes/1")
        .json(ProcessUpdate {
            name: "Changed title",
        })
        .assert_success()
        .json::<ProcessData>();

    assert_eq!(data, ProcessData {
        id: 1,
        name: "Changed title".into(),
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_updating_process_requires_permissions(mut client: Client) {
    client.put("/api/v1/processes/1")
        .json(ProcessUpdate {
            name: "Changed title",
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

// TODO: Deleting processes is not yet implemented.
#[adaptarr::test(session(r#for = "user@adaptarr.test", elevated = true))]
#[ignore]
fn api_delete_process(mut client: Client) {
    client.delete("/api/v1/processes/1")
        .send()
        .assert_success();

    client.get("/api/v1/processes/1")
        .send()
        .assert_error(StatusCode::NOT_FOUND, "");

    let data = client.get("/api/v1/processes")
        .send()
        .json::<Vec<ProcessData>>();

    assert_eq!(data, []);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_deleting_process_requires_permissions(mut client: Client) {
    client.delete("/api/v1/processes/1")
        .send()
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_process_versions(mut client: Client) {
    let data = client.get("/api/v1/processes/1/versions")
        .send()
        .assert_success()
        .json::<Vec<VersionData>>();

    assert_eq!(data, [
        VersionData {
            id: 1,
            name: "Test process".into(),
            version: data[0].version,
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_process_version(mut client: Client) {
    let data = client.get("/api/v1/processes/1/versions/1")
        .send()
        .assert_success()
        .json::<VersionData>();

    assert_eq!(data, VersionData {
        id: 1,
        name: "Test process".into(),
        version: data.version,
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_version_structure(mut client: Client) {
    let data = client.get("/api/v1/processes/1/versions/1/structure")
        .send()
        .assert_success()
        .json::<structure::Process>();

    assert_eq!(data, *TEST_PROCESS_DB);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test", elevated = true))]
fn api_create_process_version(mut client: Client) {
    let mut new = TEST_PROCESS.clone();
    new.name = "New version".into();
    new.slots.push(structure::Slot {
        id: 1,
        name: "New slot".into(),
        role: None,
        autofill: false,
    });
    new.steps[0].slots.push(structure::StepSlot {
        slot: 1,
        permission: SlotPermission::View,
    });

    let data = client.post("/api/v1/processes/1/versions")
        .json(&new)
        .assert_success()
        .json::<VersionData>();

    assert_eq!(data, VersionData {
        id: 2,
        name: "New version".into(),
        version: data.version,
    });

    let data = client.get("/api/v1/processes/1/structure")
        .send()
        .assert_success()
        .json::<structure::Process>();

    // There is already one slot, so new ones will be numbered starting with 2
    new.slots[0].id = 2;
    new.slots[1].id = 3;
    // There are already two steps, so new ones will be numbered starting with 3.
    new.steps[0].id = 3;
    new.steps[1].id = 4;

    compare_structures(&data, &new);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_creating_process_version_requires_permissions(mut client: Client) {
    client.post("/api/v1/processes/1/versions")
        .json(&*TEST_PROCESS)
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

fn compare_structures(left: &structure::Process, right: &structure::Process) {
    if left.name != right.name
    || left.steps[left.start].id != right.steps[right.start].id {
        panic!(
            "assertion failed: `compare_structures(left, right)`\n\
              left:  `Process {{ name: {:?}, start: {:?}, .. }}`,\n\
              right: `Process {{ name: {:?}, start: {:?}, .. }}`",
            left.name,
            left.steps[left.start].id,
            right.name,
            right.steps[right.start].id,
        );
    }

    let mut left_slots = left.slots.clone();
    left_slots.sort_by_key(|slot| slot.id);

    let mut right_slots = right.slots.clone();
    right_slots.sort_by_key(|slot| slot.id);

    if left_slots != right_slots {
        panic!(
            "assertion failed: `compare_structures(left, right)`\n\
              left:  `Process {{ slots: {:?}, .. }}`,\n\
              right: `Process {{ slots: {:?}, .. }}`",
            left_slots,
            right_slots,
        );
    }

    let mut left_steps = left.steps.clone();
    sort_steps(&mut left_steps, &left);

    let mut right_steps = right.steps.clone();
    sort_steps(&mut right_steps, &right);

    if left_steps != right_steps {
        panic!(
            "assertion failed: `compare_structures(left, right)`\n\
              left:  `Process {{ steps: {:?}, .. }}`,\n\
              right: `Process {{ steps: {:?}, .. }}`",
            left_steps,
            right_steps,
        );
    }
}

fn sort_steps(steps: &mut [structure::Step], original: &structure::Process) {
    steps.sort_by_key(|step| step.id);

    for step in steps {
        step.links.sort_by_key(|link| (link.to, link.slot));

        for slot in &mut step.slots {
            slot.slot = original.slots[slot.slot].id as usize;
        }

        for link in &mut step.links {
            link.to = original.steps[link.to].id as usize;
            link.slot = original.slots[link.slot].id as usize;
        }
    }
}
