//! Tests for user management (users, roles, permissions, etc.).

use actix_web::http::StatusCode;
use adaptarr::{
    models::{User, Role},
    permissions::PermissionBits,
};
use failure::Fallible;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

mod common;

use self::common::{Client, Connection, Pooled};

#[adaptarr::test_database]
fn setup_db(db: &Connection) -> Fallible<()> {
    Role::create(
        db,
        "Role",
        PermissionBits::EDIT_MODULE,
    )?;

    Role::create(
        db,
        "Second Role",
        PermissionBits::EDIT_USER_PERMISSIONS | PermissionBits::ASSIGN_MODULE,
    )?;

    User::create(
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
        "user2@adaptarr.test",
        "Second User",
        "test",
        false,
        "en",
        PermissionBits::EDIT_USER_PERMISSIONS | PermissionBits::EDIT_MODULE,
    )?;

    User::create(
        db,
        "administrator@adaptarr.test",
        "Administrator",
        "test",
        true,
        "en",
        PermissionBits::empty(),
    )?;

    Ok(())
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct RoleData<'a> {
    id: i32,
    name: Cow<'a, str>,
    permissions: Option<PermissionBits>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct UserData<'a> {
    id: i32,
    name: Cow<'a, str>,
    is_super: bool,
    language: Cow<'a, str>,
    permissions: Option<PermissionBits>,
    role: Option<RoleData<'a>>,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_users(mut client: Client) {
    let data = client.get("/api/v1/users")
        .send()
        .assert_success()
        .json::<Vec<UserData>>();

    assert_eq!(data, [
        UserData {
            id: 1,
            name: "User".into(),
            is_super: false,
            language: "en".into(),
            permissions: None,
            role: None,
        },
        UserData {
            id: 2,
            name: "Second User".into(),
            is_super: false,
            language: "en".into(),
            permissions: None,
            role: None,
        },
        UserData {
            id: 3,
            name: "Administrator".into(),
            is_super: true,
            language: "en".into(),
            permissions: None,
            role: None,
        },
    ]);
}

// FIXME: Not all bits are returned from the server (unallocated group bits are
// excluded).
#[adaptarr::test(session(r#for = "user2@adaptarr.test"))]
#[ignore]
fn api_list_of_users_with_permissions(mut client: Client) {
    let data = client.get("/api/v1/users")
        .send()
        .assert_success()
        .json::<Vec<UserData>>();

    assert_eq!(data, [
        UserData {
            id: 1,
            name: "User".into(),
            is_super: false,
            language: "en".into(),
            permissions: Some(PermissionBits::empty()),
            role: None,
        },
        UserData {
            id: 2,
            name: "Second User".into(),
            is_super: false,
            language: "en".into(),
            permissions: Some(PermissionBits::EDIT_USER_PERMISSIONS
                | PermissionBits::EDIT_MODULE),
            role: None,
        },
        UserData {
            id: 3,
            name: "Administrator".into(),
            is_super: true,
            language: "en".into(),
            permissions: Some(PermissionBits::all()),
            role: None,
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_particular_user(mut client: Client) {
    let data = client.get("/api/v1/users/2")
        .send()
        .assert_success()
        .json::<UserData>();

    assert_eq!(data, UserData {
        id: 2,
        name: "Second User".into(),
        is_super: false,
        language: "en".into(),
        permissions: None,
        role: None,
    });
}

#[adaptarr::test(session(r#for = "user2@adaptarr.test"))]
fn api_particular_user_with_permissions(mut client: Client) {
    let data = client.get("/api/v1/users/1")
        .send()
        .assert_success()
        .json::<UserData>();

    assert_eq!(data, UserData {
        id: 1,
        name: "User".into(),
        is_super: false,
        language: "en".into(),
        permissions: Some(PermissionBits::empty()),
        role: None,
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_current_user(mut client: Client) {
    let data = client.get("/api/v1/users/me")
        .send()
        .assert_success()
        .json::<UserData>();

    assert_eq!(data, UserData {
        id: 1,
        name: "User".into(),
        is_super: false,
        language: "en".into(),
        permissions: Some(PermissionBits::empty()),
        role: None,
    });
}

#[derive(Serialize)]
struct InviteParams<'a> {
    email: &'a str,
    language: &'a str,
}

#[adaptarr::test(
    session(
        r#for = "administrator@adaptarr.test",
        permissions = PermissionBits::INVITE_USER,
        elevated = true,
    ),
)]
fn send_invitation(mut client: Client) {
    client.post("/api/v1/users/invite")
        .json(InviteParams {
            email: "test3@adaptarr.test",
            language: "en",
        })
        .assert_success();
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn invitation_can_only_be_created_with_permissions(mut client: Client) {
    client.post("/api/v1/users/invite")
        .json(InviteParams {
            email: "test3@adaptarr.test",
            language: "en",
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[derive(Serialize)]
struct UserUpdate<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<PermissionBits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<Option<i32>>,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn user_can_change_their_language(mut client: Client) {
    let data = client.put("/api/v1/users/me")
        .json(UserUpdate {
            language: Some("pl"),
            permissions: None,
            role: None,
        })
        .assert_success()
        .json::<UserData>();

    assert_eq!(data, UserData {
        id: 1,
        name: "User".into(),
        is_super: false,
        language: "pl".into(),
        permissions: Some(PermissionBits::empty()),
        role: None,
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test", elevated = true))]
fn users_role_permissions_can_only_be_changed_with_permissions(mut client: Client) {
    client.put("/api/v1/users/1")
        .json(UserUpdate {
            language: None,
            permissions: Some(PermissionBits::all()),
            role: None,
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(
    session(
        r#for = "user2@adaptarr.test",
        permissions = PermissionBits::EDIT_USER_PERMISSIONS,
        elevated = true,
    ),
)]
fn change_users_role_permissions(mut client: Client) {
    client.put("/api/v1/users/1")
        .json(UserUpdate {
            language: None,
            permissions: Some(PermissionBits::all()),
            role: None,
        })
        .assert_success();
}

#[derive(Serialize)]
struct PasswordChangeRequest<'a> {
    current: &'a str,
    new: &'a str,
    new2: &'a str,
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn change_users_password(db: Pooled, mut client: Client) -> Fallible<()> {
    let user = User::by_email(&*db, "user@adaptarr.test")?;

    client.put("/api/v1/users/me/password")
        .json(PasswordChangeRequest {
            current: "bad-password",
            new: "test",
            new2: "test",
        })
        .assert_error(StatusCode::BAD_REQUEST, "user:authenticate:bad-password");

    let user2 = User::by_email(&*db, "user@adaptarr.test")?;
    assert_eq!(user.password, user2.password, "Password should not have changed");

    client.put("/api/v1/users/me/password")
        .json(PasswordChangeRequest {
            current: "test",
            new: "passwords",
            new2: "differ",
        })
        .assert_error(StatusCode::BAD_REQUEST, "user:password:bad-confirmation");

    let user2 = User::by_email(&*db, "user@adaptarr.test")?;
    assert_eq!(user.password, user2.password, "Password should not have changed");

    client.put("/api/v1/users/me/password")
        .json(PasswordChangeRequest {
            current: "test",
            new: "new",
            new2: "new",
        })
        .assert_success();

    let user2 = User::by_email(&*db, "user@adaptarr.test")?;
    assert_ne!(user.password, user2.password, "Password should have changed");

    Ok(())
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_list_of_roles(mut client: Client) {
    let data = client.get("/api/v1/roles")
        .send()
        .assert_success()
        .json::<Vec<RoleData>>();

    assert_eq!(data, [
        RoleData {
            id: 1,
            name: "Role".into(),
            permissions: None,
        },
        RoleData {
            id: 2,
            name: "Second Role".into(),
            permissions: None,
        },
    ]);
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test"))]
fn api_list_of_roles_with_permissions(mut client: Client) {
    let data = client.get("/api/v1/roles")
        .send()
        .assert_success()
        .json::<Vec<RoleData>>();

    assert_eq!(data, [
        RoleData {
            id: 1,
            name: "Role".into(),
            permissions: Some(PermissionBits::EDIT_MODULE),
        },
        RoleData {
            id: 2,
            name: "Second Role".into(),
            permissions: Some(PermissionBits::EDIT_USER_PERMISSIONS
                | PermissionBits::ASSIGN_MODULE),
        },
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn api_get_specific_role(mut client: Client) {
    let data = client.get("/api/v1/roles/1")
        .send()
        .assert_success()
        .json::<RoleData>();

    assert_eq!(data, RoleData {
        id: 1,
        name: "Role".into(),
        permissions: None,
    });
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test"))]
fn api_get_specific_role_with_permissions(mut client: Client) {
    let data = client.get("/api/v1/roles/2")
        .send()
        .assert_success()
        .json::<RoleData>();

    assert_eq!(data, RoleData {
        id: 2,
        name: "Second Role".into(),
        permissions: Some(PermissionBits::EDIT_USER_PERMISSIONS
            | PermissionBits::ASSIGN_MODULE),
    });
}

#[derive(Serialize)]
struct NewRole<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<PermissionBits>
}

#[adaptarr::test(
    session(
        r#for = "administrator@adaptarr.test"
        permissions = PermissionBits::EDIT_ROLE,
        elevated = true,
    ),
)]
fn create_role(mut client: Client) {
    let data = client.post("/api/v1/roles")
        .json(NewRole {
            name: "Test",
            permissions: Some(PermissionBits::EDIT_BOOK),
        })
        .assert_success()
        .json::<RoleData>();

    assert_eq!(data, RoleData {
        id: 3,
        name: "Test".into(),
        permissions: Some(PermissionBits::EDIT_BOOK),
    });
}

#[adaptarr::test(session(r#for = "user2@adaptarr.test", elevated = true))]
fn creating_role_requires_permission(mut client: Client) {
    client.post("/api/v1/roles")
        .json(NewRole {
            name: "Test",
            permissions: Some(PermissionBits::EDIT_BOOK),
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[derive(Serialize)]
struct RoleUpdate<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<PermissionBits>,
}

#[adaptarr::test(
    session(
        r#for = "administrator@adaptarr.test",
        permissions = PermissionBits::EDIT_ROLE,
        elevated = true,
    ),
)]
fn update_role(mut client: Client) {
    let data = client.put("/api/v1/roles/2")
        .json(RoleUpdate {
            name: Some("Renamed"),
            permissions: None,
        })
        .assert_success()
        .json::<RoleData>();

    assert_eq!(data, RoleData {
        id: 2,
        name: "Renamed".into(),
        permissions: Some(PermissionBits::EDIT_USER_PERMISSIONS
            | PermissionBits::ASSIGN_MODULE),
    });

    let data = client.put("/api/v1/roles/2")
        .json(RoleUpdate {
            name: None,
            permissions: Some(PermissionBits::EDIT_BOOK),
        })
        .assert_success()
        .json::<RoleData>();

    assert_eq!(data, RoleData {
        id: 2,
        name: "Renamed".into(),
        permissions: Some(PermissionBits::EDIT_BOOK),
    });
}

#[adaptarr::test(session(r#for = "user2@adaptarr.test", elevated = true))]
fn updating_role_requires_permission(mut client: Client) {
    client.put("/api/v1/roles/2")
        .json(RoleUpdate {
            name: Some("Renamed"),
            permissions: None,
        })
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}

#[adaptarr::test(
    session(
        r#for = "administrator@adaptarr.test",
        permissions = PermissionBits::EDIT_ROLE,
        elevated = true,
    ),
)]
fn delete_role(mut client: Client) {
    client.delete("/api/v1/roles/1")
        .send()
        .assert_success();

    let data = client.get("/api/v1/roles")
        .send()
        .json::<Vec<RoleData>>();

    assert_eq!(data, [
        RoleData {
            id: 2,
            name: "Second Role".into(),
            permissions: Some(PermissionBits::EDIT_USER_PERMISSIONS
                | PermissionBits::ASSIGN_MODULE),
        }
    ]);
}

#[adaptarr::test(session(r#for = "user@adaptarr.test", elevated = true))]
fn deleting_role_requires_permission(mut client: Client) {
    client.delete("/api/v1/roles/1")
        .send()
        .assert_error(StatusCode::FORBIDDEN, "user:insufficient-permissions");
}
