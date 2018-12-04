use super::schema::*;

#[derive(Associations, Clone, Debug, Identifiable, Queryable)]
pub struct User {
    pub id: i32,
    /// User's email address. We use this for identification (e.g. when logging
    /// into the system) and communication.
    pub email: String,
    /// User's display name. This is visible to other users.
    pub name: String,
    /// Hash of password, currently Argon2.
    pub password: Vec<u8>,
    /// Salt used for hashing password.
    pub salt: Vec<u8>,
    /// Is this user an administrator?
    pub is_super: bool,
}

#[derive(Clone, Copy, Debug, Insertable)]
#[table_name = "users"]
pub struct NewUser<'a> {
    pub name: &'a str,
    pub email: &'a str,
    pub password: &'a [u8],
    pub salt: &'a [u8],
    pub is_super: bool,
}
