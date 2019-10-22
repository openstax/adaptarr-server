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
    Optional,
    PermissionBits,
    Role,
    Team,
    TeamPermissions,
    TeamResource,
    User,
    UserAuthenticateError,
    UserPublicParams,
    db::Connection,
};
use adaptarr_web::{FormOrJson, Database, session::{Session, Normal}};
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
    let user = session.user(&db)?;

    let (users, teams) = if session.is_elevated {
        (User::all(&db)?, None)
    } else {
        let teams = user.get_team_ids(&db)?;
        let users = User::by_team(&db, &teams)?;

        (users, Some(teams))
    };

    let params = UserPublicParams {
        include_permissions: session.is_elevated,
        include_teams: teams,
    };

    Ok(Json(users.get_public_full(&db, &params)?))
}

#[derive(Deserialize)]
struct InviteParams {
    email: String,
    language: LanguageTag,
    role: Option<i32>,
    team: i32,
    permissions: TeamPermissions,
}

#[derive(ApiError, Debug, Fail)]
#[api(status = "BAD_REQUEST", code = "locale:not-found")]
#[fail(display = "No such locale")]
struct NoSuchLocaleError;

#[derive(ApiError, Debug, Fail)]
#[api(status = "FORBIDDEN", code = "user:session:rejected")]
#[fail(display = "Can't invite external user")]
struct InviteExternalError;

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
    db: Database,
    i18n: Data<I18n<'static>>,
    session: Session,
    params: FormOrJson<InviteParams>,
) -> Result<HttpResponse> {
    let team = Team::by_id(&db, params.team)?;
    let role = Option::<Role>::by_id(&db, params.role)?;
    let invitee = User::by_email(&db, &params.email).optional()?;

    let permissions = if session.is_elevated {
        params.permissions
    } else {
        let user = session.user(&db)?;
        let member = team.get_member(&db, &user)?;

        member.permissions().require(TeamPermissions::ADD_MEMBER)?;

        params.permissions & member.permissions()
    };

    let invite = match invitee {
        Some(invitee) => Invite::create_for_existing(
            &db, team, role.as_ref(), permissions, invitee)?,
        None if session.is_elevated => Invite::create(
            &db, team, &params.email, role.as_ref(), params.permissions)?,
        None => return Err(InviteExternalError.into()),
    };

    let locale = i18n.find_locale(&params.language).ok_or(NoSuchLocaleError)?;

    invite.do_send_mail(locale);

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
    let user = session.user(&db)?;

    let teams = if session.is_elevated {
        None
    } else {
        Some(user.get_team_ids(&db)?)
    };

    let params = UserPublicParams {
        include_permissions: id.is_current() || session.is_elevated,
        include_teams: teams,
    };

    Ok(Json(id.get_user(&db, &session)?.get_public_full(&db, &params)?))
}

#[derive(Deserialize)]
struct UserUpdate {
    language: Option<LanguageTag>,
    name: Option<String>,
    is_support: Option<bool>,
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
    let mut user = id.get_user(&db, &session)?;

    let db = &db;
    db.transaction::<_, Error, _>(|| {
        if let Some(language) = form.language {
            if !id.is_current() && !session.is_elevated {
                unimplemented!()
            }

            let locale = i18n.find_locale(&language).ok_or(NoSuchLocaleError)?;
            user.set_language(db, &locale.code)?;
        }

        if let Some(name) = form.name {
            if !id.is_current() && !session.is_elevated {
                unimplemented!()
            }

            user.set_name(db, &name)?;
        }

        if let Some(is_support) = form.is_support {
            if !session.is_elevated {
                return Err(Forbidden.into());
            }

            user.set_is_support(db, is_support)?;
        }

        Ok(())
    })?;

    let params = UserPublicParams {
        include_permissions: id.is_current() || session.is_elevated,
        include_teams: if session.is_elevated {
            None
        } else {
            Some(user.get_team_ids(&db)?)
        },
    };

    Ok(Json(user.get_public_full(&db, &params)?))
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
    let user = id.get_user(&db, &session)?;
    let mut drafts = Draft::all_of(&db, user.id())?;

    if !id.is_current() {
        let teams = session.user(&db)?.get_team_ids(&db)?;
        drafts.retain(|draft| teams.contains(&draft.team_id()));
    }

    Ok(Json(drafts.get_public_full(&db, &user.id)?))
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
    })
}

#[derive(ApiError, Debug, Fail)]
#[api(status = "FORBIDDEN", code = "user:insufficient-permissions")]
#[fail(display = "insufficient permissions to perform this action")]
struct Forbidden;

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
