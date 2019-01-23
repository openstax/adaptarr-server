use actix_web::{
    App,
    Either,
    Form,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    http::Method,
};
use serde::de::{Deserialize, Deserializer};

use crate::models::{
    Invite,
    user::{User, PublicData, UserAuthenticateError},
};
use super::{
    Error,
    RouteExt,
    RouterExt,
    State,
    session::{Session, Normal, ElevatedSession},
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .api_route("/users", Method::GET, list_users)
        .scope("/users", |scope| scope
        .api_route("/invite", Method::POST, create_invitation)
        .resource("/{id}", |r| {
            r.get().api_with(get_user);
            r.put().f(modify_user);
        })
        .api_route("/me/password", Method::PUT, modify_password))
}

#[derive(Debug, Deserialize)]
pub struct InviteParams {
    email: String,
}

#[derive(Serialize)]
struct InviteTemplate {
    url: String,
}

/// Get list of all users.
///
/// ## Method
///
/// ```
/// GET /users
/// ```
pub fn list_users((
    state,
    _session,
): (
    actix_web::State<State>,
    Session,
)) -> Result<Json<Vec<PublicData>>, Error> {
    let db = state.db.get()?;

    User::all(&*db)
        .map(|v| v.into_iter().map(|u| u.get_public()).collect())
        .map(Json)
        .map_err(Into::into)
}

/// Create an invitation.
///
/// This endpoint is only accessible in an elevated session.
///
/// ## Method
///
/// ```
/// POST /users/invite
/// ```
pub fn create_invitation((
    state,
    _session,
    params,
): (
    actix_web::State<State>,
    ElevatedSession,
    Json<InviteParams>,
)) -> Result<HttpResponse, Error> {
    let db = state.db.get()?;
    let invite = Invite::create(&*db, &params.email)?;

    let code = invite.get_code(&state.config);
    // TODO: get URL from Actix.
    let url = format!(
        "https://{}/register?invite={}",
        &state.config.server.domain,
        code,
    );

    state.mailer.send(
        "invite", params.email.as_str(), "Invitation", &InviteTemplate { url });

    Ok(HttpResponse::Ok().finish())
}

/// Get user information.
///
/// ## Method
///
/// ```
/// GET /users/:id
/// ```
pub fn get_user((
    state,
    session,
    path,
): (
    actix_web::State<State>,
    Session,
    Path<UserId>,
)) -> Result<Json<PublicData>, Error> {
    let user = path.get_user(&*state, &session)?;

    Ok(Json(user.get_public()))
}

/// Update user information.
///
/// ## Method
///
/// ```
/// PUT /users/:id
/// ```
pub fn modify_user(_req: &HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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
/// ```
/// PUT /users/me/password
/// ```
pub fn modify_password((
    req,
    state,
    session,
    form,
): (
    HttpRequest<State>,
    actix_web::State<State>,
    Session,
    Either<Form<PasswordChangeRequest>, Json<PasswordChangeRequest>>,
)) -> Result<HttpResponse, Error> {
    let db = state.db.get()?;
    let mut user = User::by_id(&*db, session.user)?;

    let form = match form {
        Either::A(form) => form.into_inner(),
        Either::B(json) => json.into_inner(),
    };

    if !user.check_password(&form.current) {
        return Err(UserAuthenticateError::BadPassword.into());
    }

    if form.new != form.new2 {
        return Err(UserAuthenticateError::BadPassword.into());
    }

    user.change_password(&*db, &form.new)?;

    // Changing password invalidates all sessions.
    Session::<Normal>::create(&req, user.id, false);

    Ok(HttpResponse::Ok().finish())
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

    pub fn get_user(&self, state: &State, session: &Session) -> Result<User, super::Error> {
        let db = state.db.get()?;
        User::by_id(&*db, self.as_id(&session))
            .map_err(Into::into)
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
