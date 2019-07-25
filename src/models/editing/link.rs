use diesel::{Connection as _, prelude::*, result::Error as DbError};
use failure::Fail;
use serde::Serialize;

use crate::{
    ApiError,
    audit,
    db::{
        Connection,
        models as db,
        schema::edit_process_links,
    },
};

/// A transition between two editing steps.
///
/// See [module description][super] for details.
#[derive(Debug)]
pub struct Link {
    data: db::EditProcessLink,
}

/// A subset of link's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub to: i32,
    pub name: String,
    pub slot: i32,
}

impl Link {
    /// Construct `Link` from its database counterpart.
    pub(super) fn from_db(data: db::EditProcessLink) -> Link {
        Link { data }
    }

    /// Unpack database data.
    pub fn into_db(self) -> db::EditProcessLink {
        self.data
    }

    /// Get the public portion of this module's data.
    pub fn get_public(&self) -> PublicData {
        let db::EditProcessLink { to, ref name, slot, .. } = self.data;

        PublicData {
            to, slot,
            name: name.clone(),
        }
    }

    /// Set link's name.
    pub fn set_name(&mut self, db: &Connection, name: &str)
    -> Result<(), DbError> {
        db.transaction(|| {
            audit::log_db(
                db, "steps", self.data.from, "set-link-name", LogSetName {
                    slot: self.data.slot,
                    to: self.data.to,
                    name,
                });

            self.data = diesel::update(&self.data)
                .set(edit_process_links::name.eq(name))
                .get_result(db)?;

            Ok(())
        })
    }
}

impl std::ops::Deref for Link {
    type Target = db::EditProcessLink;

    fn deref(&self) -> &db::EditProcessLink {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindLinkError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// No link found matching given criteria.
    #[fail(display = "No such link")]
    #[api(code = "edit-process:link:not-found", status = "NOT_FOUND")]
    NotFound,
}

impl_from! { for FindLinkError ;
    DbError => |e| FindLinkError::Database(e),
}

#[derive(Serialize)]
struct LogSetName<'a> {
    slot: i32,
    to: i32,
    name: &'a str,
}
