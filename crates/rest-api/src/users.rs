use actix_web::{
    HttpRequest,
    HttpResponse,
    http::StatusCode,
    web::{self, Data, Json, Path, ServiceConfig},
};
use adaptarr_error::{ApiError, Error};
use adaptarr_i18n::{I18n, LanguageTag};
use adaptarr_models::{
    Draft,
    Invite,
    Model,
    PermissionBits,
    Role,
    User,
    UserAuthenticateError,
    UserFields,
    db::Connection,
    permissions::InviteUser,
};
use adaptarr_web::{FormOrJson, Secret, Database, session::{Session, Normal}};
use chrono::{DateTime, Utc};
use diesel::Connection as _;
use failure::Fail;
use serde::{Deserialize, Serialize, de::Deserializer};

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .route("/users", web::get().to(list_users))
        .service(web::scope("/users")
            .route("/invite", web::post().to(create_invitation))
            .service(web::resource("/{id}")
                .route(web::get().to(get_user))
                .route(web::put().to(modify_user))
            )
            .route("/{id}/drafts", web::get().to(list_user_drafts))
            .route("/me/password", web::put().to(modify_password))
            .route("/me/session", web::get().to(get_session))
        )
    ;
}

/// Get list of all users.
///
/// ## Method
///
/// ```text
/// GET /users
/// ```
fn list_users(db: Database, session: Session)
-> Result<Json<Vec<<User as Model>::Public>>> {
    let permissions = session.user(&db)?.permissions(true);

    let mut fields = UserFields::empty();

    if permissions.contains(PermissionBits::EDIT_USER_PERMISSIONS) {
        fields.insert(UserFields::PERMISSIONS);
    }
    if permissions.contains(PermissionBits::EDIT_ROLE) {
        fields.insert(UserFields::ROLE_PERMISSIONS);
    }

    Ok(Json(User::all(&db)?.get_public_full(&db, &fields)?))
}

#[derive(Deserialize)]
struct InviteParams {
    email: String,
    language: LanguageTag,
    role: Option<i32>,
}

#[derive(ApiError, Debug, Fail)]
#[api(status = "BAD_REQUEST", code = "locale:not-found")]
#[fail(display = "No such locale")]
struct NoSuchLocaleError;

/// Create an invitation.
///
/// This endpoint is only accessible in an elevated session.
///
/// ## Method
///
/// ```text
/// POST /users/invite
/// ```
fn create_invitation(
    req: HttpRequest,
    db: Database,
    i18n: Data<I18n<'static>>,
    secret: Data<Secret>,
    _: Session<InviteUser>,
    params: FormOrJson<InviteParams>,
) -> Result<HttpResponse> {
    let locale = i18n.find_locale(&params.language).ok_or(NoSuchLocaleError)?;

    let role = match params.role {
        None => None,
        Some(id) => Some(Role::by_id(&db, id)?),
    };
    let invite = Invite::create(&db, &params.email, role.as_ref())?;

    let code = invite.get_code(&secret);
    let mut url = req.url_for_static("register")?;
    url.query_pairs_mut().append_pair("invite", &code);

    invite.do_send_mail(url.as_str(), locale);

    Ok(HttpResponse::new(StatusCode::ACCEPTED))
}

/// Get user information.
///
/// ## Method
///
/// ```text
/// GET /users/:id
/// ```
fn get_user(db: Database, session: Session, id: Path<UserId>)
-> Result<Json<<User as Model>::Public>> {
    let permissions = session.user(&db)?.permissions(true);
    let mut fields = UserFields::empty();

    if id.is_current() {
        fields.insert(UserFields::PERMISSIONS | UserFields::ROLE_PERMISSIONS);
    } else if permissions.contains(PermissionBits::EDIT_USER_PERMISSIONS) {
        fields.insert(UserFields::PERMISSIONS);
    }
    if permissions.contains(PermissionBits::EDIT_ROLE) {
        fields.insert(UserFields::ROLE_PERMISSIONS);
    }

    Ok(Json(id.get_user(&db, &session)?.get_public_full(&db, &fields)?))
}

#[derive(Deserialize)]
struct UserUpdate {
    language: Option<LanguageTag>,
    permissions: Option<PermissionBits>,
    #[serde(default, deserialize_with = "adaptarr_util::de_optional_null")]
    role: Option<Option<i32>>,
    name: Option<String>,
}

/// Update user information.
///
/// ## Method
///
/// ```text
/// PUT /users/:id
/// ```
fn modify_user(
    db: Database,
    i18n: Data<I18n>,
    session: Session,
    id: Path<UserId>,
    form: FormOrJson<UserUpdate>,
) -> Result<Json<<User as Model>::Public>> {
    let form = form.into_inner();
    let permissions = session.permissions();
    let mut user = id.get_user(&db, &session)?;

    let db = &db;
    db.transaction::<_, Error, _>(|| {
        if let Some(language) = form.language {
            if !id.is_current() {
                permissions.require(PermissionBits::EDIT_USER)?;
            }

            let locale = i18n.find_locale(&language).ok_or(NoSuchLocaleError)?;
            user.set_language(db, &locale.code)?;
        }

        if let Some(name) = form.name {
            if !id.is_current() {
                permissions.require(PermissionBits::EDIT_USER)?;
            }

            user.set_name(db, &name)?;
        }

        if let Some(new_perms) = form.permissions {
            permissions.require(PermissionBits::EDIT_USER_PERMISSIONS)?;
            user.set_permissions(db, new_perms)?;
        }

        if let Some(role) = form.role {
            permissions.require(PermissionBits::ASSIGN_ROLE)?;
            let role = role.map(|id| Role::by_id(db, id))
                .transpose()?;

            user.set_role(db, role.as_ref())?;
        }

        Ok(())
    })?;

    let mut fields = UserFields::empty();

    if id.is_current()  {
        fields.insert(UserFields::PERMISSIONS | UserFields::ROLE_PERMISSIONS);
    } else if permissions.contains(PermissionBits::EDIT_USER_PERMISSIONS) {
        fields.insert(UserFields::PERMISSIONS);
    }
    if permissions.contains(PermissionBits::EDIT_ROLE) {
        fields.insert(UserFields::ROLE_PERMISSIONS);
    }

    Ok(Json(user.get_public_full(&db, &fields)?))
}

/// Get list of drafts a given user has access to.
///
/// ## Method
///
/// ```text
/// GET /users/:id/drafts
/// ```
fn list_user_drafts(db: Database, session: Session, id: Path<UserId>)
-> Result<Json<Vec<<Draft as Model>::Public>>> {
    if !id.is_current() {
        session.user(&db)?
            .permissions(true)
            .require(PermissionBits::MANAGE_PROCESS)?;
    }

    let user = id.get_user(&db, &session)?;

    Ok(Json(Draft::all_of(&db, user.id())?.get_public_full(&db, &user.id)?))
}

#[derive(Deserialize)]
struct PasswordChangeRequest {
    current: String,
    new: String,
    new2: String,
}

#[derive(ApiError, Debug, Fail)]
enum PasswordChangeError {
    #[api(status = "BAD_REQUEST", code = "user:password:bad-confirmation")]
    #[fail(display = "password and confirmation don't match")]
    PasswordsDontMatch,
}

/// Change password.
///
/// ## Method
///
/// ```text
/// PUT /users/me/password
/// ```
fn modify_password(
    req: HttpRequest,
    db: Database,
    session: Session,
    form: FormOrJson<PasswordChangeRequest>,
) -> Result<HttpResponse> {
    let form = form.into_inner();
    let mut user = User::by_id(&db, session.user)?;

    if !user.check_password(&form.current) {
        return Err(UserAuthenticateError::BadPassword.into());
    }

    if form.new != form.new2 {
        return Err(PasswordChangeError::PasswordsDontMatch.into());
    }

    user.change_password(&db, &form.new)?;

    // Changing password invalidates all sessions.
    Session::<Normal>::create(&req, &user, false);

    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

#[derive(Debug, Serialize)]
struct SessionData {
    expires: DateTime<Utc>,
    is_elevated: bool,
    permissions: PermissionBits,
}

/// Get details about current session.
///
/// ## Method
///
/// ```text
/// GET /users/me/session
/// ```
fn get_session(session: Session) -> Json<SessionData> {
    Json(SessionData {
        expires: session.expires,
        is_elevated: session.is_elevated,
        permissions: session.permissions(),
    })
}

/// ID of a user, can be either a number of a string `"me"`.
enum UserId {
    /// Same as as `ById` with ID of the current user. Determined by active
    /// session.
    Current,
    /// Explicit ID.
    ById(i32),
}

impl UserId {
    /// Convert a path parameter into an actual ID.
    pub fn get_id<P>(&self, session: &Session<P>) -> i32 {
        match *self {
            UserId::Current => session.user,
            UserId::ById(id) => id,
        }
    }

    pub fn get_user<P>(
        &self,
        db: &Connection,
        session: &Session<P>,
    ) -> adaptarr_models::FindModelResult<User> {
        User::by_id(&db, self.get_id(&session))
    }

    /// Is this a [`UserId::Current`]?
    pub fn is_current(&self) -> bool {
        match *self {
            UserId::Current => true,
            _ => false,
        }
    }
}

// We need to implement it manually, as untagged unions are not supported
// by Path.
impl<'de> Deserialize<'de> for UserId {
    fn deserialize<D>(d: D) -> std::result::Result<UserId, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserializing from a path requires percent-decoding, which produces
        // a String for each part and, as those are temporary, there's nothing
        // we could borrow `&str` from.
        let v: String = Deserialize::deserialize(d)?;

        if v == "me" {
            return Ok(UserId::Current);
        }

        v.parse()
            .map(UserId::ById)
            .map_err(|_| serde::de::Error::custom(
                "data was neither a number nor a valid string"))
    }
}
