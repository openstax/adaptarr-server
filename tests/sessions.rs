use actix_web::http::{Cookie, StatusCode, header::LOCATION};
use adaptarr::{
    models::{User, Role},
    permissions::PermissionBits,
};
use chrono::{Duration, Utc};
use diesel::prelude::*;
use failure::Fallible;

mod common;

use self::common::{Client, Connection, Session, Pooled, models::*};

#[adaptarr::test_database]
fn setup_db(db: &Connection) -> Fallible<()> {
    let role = Role::create(
        db,
        "Role",
        PermissionBits::EDIT_MODULE | PermissionBits::MANAGE_PROCESS,
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

    User::create(
        db,
        "user@adaptarr.test",
        "User",
        "test",
        false,
        "en",
        PermissionBits::EDIT_MODULE,
    )?;

    User::create(
        db,
        "user2@adaptarr.test",
        "Second User",
        "test",
        false,
        "en",
        PermissionBits::EDIT_BOOK,
    )?.set_role(db, Some(&role))?;

    Ok(())
}

/// Decode a session cookie.
fn decode_sesid(cookie: &Cookie) -> Fallible<i32> {
    let mut data = base64::decode(cookie.value())?;
    Ok(adaptarr::utils::unseal(&[0; 32], &mut data)?)
}

#[adaptarr::test]
fn login_page_creates_session(db: Pooled, mut client: Client) -> Fallible<()> {
    const NEXT: &str = "adaptarr.test/test/page";

    let rsp = client.post("/login")
        .form(LoginCredentials {
            email: "administrator@adaptarr.test",
            password: "test",
            next: Some(NEXT),
        })
        // Successful login should result in redirection to next.
        .assert_redirection();

    // Verify redirection is correct.
    assert_eq!(rsp.header(LOCATION), NEXT);

    // Verify session cookie names an existing session...
    let sesid = decode_sesid(&rsp.cookie("sesid")).expect("Invalid session cookie");

    let session = common::find_session(&*db, sesid)
        .optional()?
        .expect("Session cookie names a non-existent session");

    // ... belonging to the correct user and having correct properties.
    let user = User::by_email(&*db, "administrator@adaptarr.test").unwrap();

    assert_eq!(session.user, user.id, "Session is assigned to wrong user");
    assert_eq!(session.is_elevated, false, "Session should not be elevated");
    assert!(session.permissions().is_empty(), "Session should have no permissions");

    Ok(())
}

#[adaptarr::test]
fn elevate_page_elevates_session(db: Pooled, mut client: Client) -> Fallible<()> {
    const NEXT: &str = "adaptarr.test/test/page";

    // POST /login - create a normal session

    let rsp = client.post("/login")
        .form(LoginCredentials {
            email: "user@adaptarr.test",
            password: "test",
            next: None,
        });

    let cookie = rsp.cookie("sesid").into_owned();
    let sesid = decode_sesid(&cookie).expect("Invalid session cookie");

    // POST /elevate - elevate an existing session

    let rsp = client.post("/elevate")
        .cookie(cookie)
        .form(ElevateCredentials {
            password: "test",
            next: Some(NEXT),
            action: None,
        })
        // Successful elevation should result in redirection to next
        .assert_redirection();

    // Verify redirection is correct.
    assert_eq!(rsp.header(LOCATION), NEXT);

    // Verify previous (unelevated) session was destroyed.
    assert!(
        common::find_session(&*db, sesid).optional()?.is_none(),
        "Previous session wasn't destroyed",
    );

    // Verify that elevation created a new session...
    let new_sesid = decode_sesid(&rsp.cookie("sesid")).expect("Invalid session cookie");
    let session = common::find_session(&*db, new_sesid)
        .optional()?
        .expect("Session cookie names a non-existent session");

    // ... for the correct user and with correct properties
    let user = User::by_email(&*db, "user@adaptarr.test").unwrap();

    assert_eq!(session.user, user.id, "Session is assigned to wrong user");
    assert_eq!(session.is_elevated, true, "Session should be elevated");
    assert_eq!(
        session.permissions(),
        PermissionBits::EDIT_MODULE,
        "Session should have no permissions",
    );

    Ok(())
}

#[adaptarr::test]
fn logout_page_destroys_session(db: Pooled, mut client: Client) -> Fallible<()> {
    // POST /login

    let rsp = client.post("/login")
        .form(LoginCredentials {
            email: "user@adaptarr.test",
            password: "test",
            next: None,
        });

    let cookie = rsp.cookie("sesid").into_owned();
    let sesid = decode_sesid(&cookie)?;

    // GET /logout

    client.get("/logout")
        .cookie(cookie)
        .send()
        .assert_success();

    assert!(
        common::find_session(&*db, sesid).optional()?.is_none(),
        "Session wasn't destroyed",
    );

    Ok(())
}

#[adaptarr::test(
    session(
        r#for = "user@adaptarr.test",
        expires = Utc::now().naive_utc() - Duration::days(1),
    ),
)]
fn session_expires(db: Pooled, mut client: Client, session: Session)
-> Fallible<()> {
    client.get("/api/v1/users/me/session")
        .send()
        .assert_status(StatusCode::UNAUTHORIZED);

    assert!(
        common::find_session(&*db, session.id).optional()?.is_none(),
        "Expired session should be removed",
    );

    Ok(())
}

#[adaptarr::test(
    session(
        r#for = "administrator@adaptarr.test",
        elevated = true,
        last_used = Utc::now().naive_utc() - Duration::minutes(30),
    ),
)]
fn elevated_session_expires(mut client: Client, session: Session) {
    let data: SessionData = client.get("/api/v1/users/me/session")
        .send()
        .assert_success()
        .json();

    assert_eq!(data, SessionData {
        expires: session.expires,
        is_elevated: false,
        permissions: PermissionBits::empty(),
    });
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn normal_session_has_no_elevated_permissions(db: Pooled, mut client: Client)
-> Fallible<()> {
    let rsp = client.post("/elevate")
        .form(ElevateCredentials {
            password: "test",
            next: None,
            action: None,
        });

    let sesid = decode_sesid(&rsp.cookie("sesid"))?;
    let session = common::find_session(&*db, sesid)?;

    assert_eq!(session.is_elevated, true);
    assert_eq!(session.permissions(), PermissionBits::EDIT_MODULE);

    Ok(())
}

#[adaptarr::test(session(r#for = "administrator@adaptarr.test"))]
fn administrator_session_has_all_permissions(db: Pooled, mut client: Client)
-> Fallible<()> {
    let rsp = client.post("/elevate")
        .form(ElevateCredentials {
            password: "test",
            next: None,
            action: None,
        });

    let sesid = decode_sesid(&rsp.cookie("sesid"))?;
    let session = common::find_session(&*db, sesid)?;

    assert_eq!(session.is_elevated, true);
    assert_eq!(session.permissions(), PermissionBits::all());

    Ok(())
}

#[adaptarr::test(session(r#for = "user2@adaptarr.test"))]
fn session_includes_role_permissions(db: Pooled, mut client: Client)
-> Fallible<()> {
    let rsp = client.post("/elevate")
        .form(ElevateCredentials {
            password: "test",
            next: None,
            action: None,
        });

    let sesid = decode_sesid(&rsp.cookie("sesid"))?;
    let session = common::find_session(&*db, sesid)?;

    assert_eq!(session.is_elevated, true);
    assert_eq!(
        session.permissions(),
        (PermissionBits::EDIT_MODULE | PermissionBits::MANAGE_PROCESS
            | PermissionBits::EDIT_BOOK),
    );

    Ok(())
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn assigning_user_a_role_updates_session_permissions(
    db: Pooled,
    mut client: Client,
) -> Fallible<()> {
    let role = Role::by_id(&*db, 1)?;
    let mut user = User::by_email(&*db, "user@adaptarr.test")?;

    let rsp = client.post("/elevate")
        .form(ElevateCredentials {
            password: "test",
            next: None,
            action: None,
        });

    let sesid = decode_sesid(&rsp.cookie("sesid"))?;
    let mut session = common::find_session(&*db, sesid)?;

    assert_eq!(session.permissions(), PermissionBits::EDIT_MODULE);

    user.set_role(&*db, Some(&role))?;
    session.reload(&*db)?;

    assert_eq!(
        session.permissions(),
        (PermissionBits::EDIT_MODULE | PermissionBits::MANAGE_PROCESS),
    );

    user.set_role(&*db, None)?;
    session.reload(&*db)?;

    assert_eq!(session.permissions(), PermissionBits::EDIT_MODULE);

    Ok(())
}

#[adaptarr::test(session(r#for = "user@adaptarr.test"))]
fn changing_password_invalidates_all_sessions(db: Pooled, session: Session)
-> Fallible<()> {
    let mut user = User::by_email(&*db, "user@adaptarr.test")?;

    user.change_password(&*db, "test2")?;

    assert!(
        common::find_session(&*db, session.id).optional()?.is_none(),
        "All existing session should have been removed",
    );

    Ok(())
}
