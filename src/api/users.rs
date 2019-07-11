use actix_web::{
    App,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    http::Method,
};
use chrono::NaiveDateTime;
use failure::Fail;
use serde::{Deserialize, Serialize, de::Deserializer};

use crate::{
    ApiError,
    i18n::LanguageTag,
    mail::Mailer,
    models::{
        Invite,
        Role,
        user::{Fields, User, PublicData, UserAuthenticateError},
    },
    permissions::{InviteUser, PermissionBits},
    templates,
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    session::{Session, Normal},
    util::FormOrJson,
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .api_route("/users", Method::GET, list_users)
        .scope("/users", |scope| scope
        .api_route("/invite", Method::POST, create_invitation)
        .resource("/{id}", |r| {
            r.get().api_with(get_user);
            r.put().api_with(modify_user);
        })
        .api_route("/me/password", Method::PUT, modify_password)
        .route("/me/session", Method::GET, get_session))
}

#[derive(Debug, Deserialize)]
pub struct InviteParams {
    email: String,
    language: LanguageTag,
    role: Option<i32>,
}

/// Get list of all users.
///
/// ## Method
///
/// ```text
/// GET /users
/// ```
pub fn list_users(
    state: actix_web::State<State>,
    session: Session,
) -> Result<Json<Vec<PublicData>>, Error> {
    let db = state.db.get()?;
    let permissions = session.user(&*db)?.permissions(true);

    let mut fields = Fields::empty();

    if permissions.contains(PermissionBits::EDIT_USER_PERMISSIONS) {
        fields.insert(Fields::PERMISSIONS);
    }
    if permissions.contains(PermissionBits::EDIT_ROLE) {
        fields.insert(Fields::ROLE_PERMISSIONS);
    }

    User::all(&*db)
        .map(|v| v.into_iter().map(|u| u.get_public(fields)).collect())
        .map(Json)
        .map_err(Into::into)
}

/// Create an invitation.
///
/// This endpoint is only accessible in an elevated session.
///
/// ## Method
///
/// ```text
/// POST /users/invite
/// ```
pub fn create_invitation(
    state: actix_web::State<State>,
    _session: Session<InviteUser>,
    params: Json<InviteParams>,
) -> Result<HttpResponse, Error> {
    let locale = state.i18n.find_locale(&params.language)
        .ok_or(NoSuchLocaleError)?;

    let db = state.db.get()?;
    let role = match params.role {
        None => None,
        Some(id) => Role::by_id(&*db, id).map(Some)?,
    };
    let invite = Invite::create(&*db, &params.email, role.as_ref())?;

    let code = invite.get_code(&state.config);
    // TODO: get URL from Actix.
    let url = format!(
        "https://{}/register?invite={}",
        &state.config.server.domain,
        code,
    );

    Mailer::do_send(
        params.email.as_str(),
        "invite",
        "mail-invite-subject",
        &templates::InviteMailArgs {
            url: &url,
            email: params.email.as_str(),
        },
        locale,
    );

    Ok(HttpResponse::Ok().finish())
}

#[derive(ApiError, Debug, Fail)]
#[api(status = "BAD_REQUEST", code = "locale:not-found")]
#[fail(display = "No such locale")]
struct NoSuchLocaleError;

/// Get user information.
///
/// ## Method
///
/// ```text
/// GET /users/:id
/// ```
pub fn get_user(
    state: actix_web::State<State>,
    session: Session,
    path: Path<UserId>,
) -> Result<Json<PublicData>, Error> {
    let db = state.db.get()?;
    let user = path.get_user(&*state, &session)?;
    let permissions = session.user(&*db)?.permissions(true);

    let mut fields = Fields::empty();

    if path.is_current() {
        fields.insert(Fields::PERMISSIONS | Fields::ROLE_PERMISSIONS);
    } else if permissions.contains(PermissionBits::EDIT_USER_PERMISSIONS) {
        fields.insert(Fields::PERMISSIONS);
    }
    if permissions.contains(PermissionBits::EDIT_ROLE) {
        fields.insert(Fields::ROLE_PERMISSIONS);
    }

    Ok(Json(user.get_public(fields)))
}

#[derive(Deserialize)]
pub struct UserUpdate {
    language: Option<LanguageTag>,
    permissions: Option<PermissionBits>,
    #[serde(default, deserialize_with = "de_optional_null")]
    role: Option<Option<i32>>,
}

/// Update user information.
///
/// ## Method
///
/// ```text
/// PUT /users/:id
/// ```
pub fn modify_user(
    state: actix_web::State<State>,
    session: Session,
    id: Path<UserId>,
    form: FormOrJson<UserUpdate>,
) -> Result<Json<PublicData>, Error> {
    let db = state.db.get()?;
    let form = form.into_inner();
    let permissions = session.permissions();
    let mut user = id.get_user(&state, &session)?;

    let dbcon = &*db;
    use diesel::Connection;
    dbcon.transaction::<_, Error, _>(|| {
        if let Some(language) = form.language {
            if !id.is_current() {
                permissions.require(PermissionBits::EDIT_USER)?;
            }

            let locale = state.i18n.find_locale(&language)
                .ok_or(NoSuchLocaleError)?;
            user.set_language(dbcon, &locale.code)?;
        }

        if let Some(new_perms) = form.permissions {
            permissions.require(PermissionBits::EDIT_USER_PERMISSIONS)?;
            user.set_permissions(dbcon, new_perms)?;
        }

        if let Some(role) = form.role {
            permissions.require(PermissionBits::ASSIGN_ROLE)?;
            let role = role.map(|id| Role::by_id(dbcon, id))
                .transpose()?;

            user.set_role(dbcon, role.as_ref())?;
        }

        Ok(())
    })?;

    let mut fields = Fields::empty();

    if id.is_current()  {
        fields.insert(Fields::PERMISSIONS | Fields::ROLE_PERMISSIONS);
    } else if permissions.contains(PermissionBits::EDIT_USER_PERMISSIONS) {
        fields.insert(Fields::PERMISSIONS);
    }
    if permissions.contains(PermissionBits::EDIT_ROLE) {
        fields.insert(Fields::ROLE_PERMISSIONS);
    }

    Ok(Json(user.get_public(fields)))
}

#[derive(Debug, Deserialize)]
pub struct PasswordChangeRequest {
    current: String,
    new: String,
    new2: String,
}

/// Change password.
///
/// ## Method
///
/// ```text
/// PUT /users/me/password
/// ```
pub fn modify_password(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    session: Session,
    form: FormOrJson<PasswordChangeRequest>,
) -> Result<HttpResponse, Error> {
    let db = state.db.get()?;
    let form = form.into_inner();
    let mut user = User::by_id(&*db, session.user)?;

    if !user.check_password(&form.current) {
        return Err(UserAuthenticateError::BadPassword.into());
    }

    if form.new != form.new2 {
        return Err(PasswordChangeError::PasswordsDontMatch.into());
    }

    user.change_password(&*db, &form.new)?;

    // Changing password invalidates all sessions.
    Session::<Normal>::create(&req, &user, false);

    Ok(HttpResponse::Ok().finish())
}

#[derive(ApiError, Debug, Fail)]
enum PasswordChangeError {
    #[api(status = "BAD_REQUEST", code = "user:password:bad-confirmation")]
    #[fail(display = "password and confirmation don't match")]
    PasswordsDontMatch,
}

#[derive(Debug, Serialize)]
pub struct SessionData {
    expires: NaiveDateTime,
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
pub fn get_session(session: Session) -> Json<SessionData> {
    Json(SessionData {
        expires: session.expires,
        is_elevated: session.is_elevated,
        permissions: session.permissions(),
    })
}

/// ID of a user, can be either a number of a string `"me"`.
#[derive(Debug)]
pub enum UserId {
    /// Same as as `ById` with ID of the current user. Determined by active
    /// session.
    Current,
    /// Explicit ID.
    ById(i32),
}

impl UserId {
    /// Convert a path parameter into an actual ID.
    pub fn as_id<P>(&self, session: &Session<P>) -> i32 {
        match *self {
            UserId::Current => session.user,
            UserId::ById(id) => id,
        }
    }

    pub fn get_user<P>(&self, state: &State, session: &Session<P>) -> Result<User, super::Error> {
        let db = state.db.get()?;
        User::by_id(&*db, self.as_id(&session))
            .map_err(Into::into)
    }

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

fn de_optional_null<'de, T, D>(de: D) -> std::result::Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(de).map(Some)
}
