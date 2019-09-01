use adaptarr_error::ApiError;
use diesel::{
    Connection as _,
    prelude::*,
    result::{DatabaseErrorKind, Error as DbError},
};
use failure::Fail;
use serde::Serialize;

use crate::{
    audit,
    db::{Connection, models as db, schema::edit_process_links},
    models::{FindModelResult, Model},
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
pub struct Public {
    pub to: i32,
    pub name: String,
    pub slot: i32,
}

impl Model for Link {
    const ERROR_CATEGORY: &'static str = "edit-process:link";

    type Id = (i32, i32);
    type Database = db::EditProcessLink;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, (from, to): Self::Id)
    -> FindModelResult<Self> {
        edit_process_links::table
            .filter(edit_process_links::from.eq(from)
                .and(edit_process_links::to.eq(to)))
            .get_result(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db(data: Self::Database) -> Self {
        Link { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        (self.data.from, self.data.to)
    }

    fn get_public(&self) -> Self::Public {
        let db::EditProcessLink { to, ref name, slot, .. } = self.data;

        Public {
            to, slot,
            name: name.clone(),
        }
    }
}

impl Link {
    /// Set link's name.
    pub fn set_name(&mut self, db: &Connection, name: &str)
    -> Result<(), RenameLinkError> {
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
pub enum RenameLinkError {
    #[api(internal)]
    #[fail(display = "{}", _0)]
    Database(#[cause] DbError),
    #[api(code = "edit-process:link:name:duplicate", status = "BAD_REQUEST")]
    #[fail(display = "rename would result in a duplicate name")]
    DuplicateName,
}

impl From<DbError> for RenameLinkError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) =>
                Self::DuplicateName,
            _ => Self::Database(err),
        }
    }
}

#[derive(Serialize)]
struct LogSetName<'a> {
    slot: i32,
    to: i32,
    name: &'a str,
}
