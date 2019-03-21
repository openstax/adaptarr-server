use diesel::{prelude::*, result::{Error as DbError}};

use crate::db::{
    Connection,
    models as db,
    schema::roles,
};

/// Role a user can take.
#[derive(Debug)]
pub struct Role {
    data: db::Role,
}

/// A subset of role's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    id: i32,
    name: String,
}

impl Role {
    /// Construct `Role` from its database counterpart.
    pub(super) fn from_db(data: db::Role) -> Role {
        Role { data }
    }

    /// Get public portion of this role's data.
    pub fn get_public(&self) -> PublicData {
        let db::Role { id, ref name, .. } = self.data;

        PublicData {
            id,
            name: name.clone(),
        }
    }
}

impl std::ops::Deref for Role {
    type Target = db::Role;

    fn deref(&self) -> &db::Role {
        &self.data
    }
}
