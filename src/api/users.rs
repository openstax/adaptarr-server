use actix_web::{
    App,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    error::{Error, ErrorInternalServerError},
    http::Method,
};
use serde::de::{Deserialize, Deserializer};

use crate::models::user::{User, PublicData};
use super::{
    State,
    session::Session,
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app.scope("/users", |scope| scope
        .route("/invite", Method::POST, create_invitation)
        .resource("/{id}", |r| {
            r.get().with(get_user);
            r.put().f(modify_user);
        })
        .route("/me/password", Method::PUT, modify_password))
}

/// Create an invitation.
///
/// ## Method
///
/// ```
/// POST /users/invite
/// ```
pub fn create_invitation(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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

/// Change password.
///
/// ## Method
///
/// ```
/// PUT /users/me/password
/// ```
pub fn modify_password(_req: HttpRequest<State>) -> HttpResponse {
    unimplemented!()
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

    pub fn get_user(&self, state: &State, session: &Session) -> Result<User, Error> {
        let db = state.db.get()
            .map_err(|e| ErrorInternalServerError(e.to_string()))?;
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
