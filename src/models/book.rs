use diesel::{
    prelude::*,
    result::Error as DbError,
};

use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::books,
};

/// A book is a collection of modules and their structure.
#[derive(Debug)]
pub struct Book {
    data: db::Book,
}

impl Book {
    /// Find a book by ID.
    pub fn by_id(dbconn: &Connection, id: Uuid) -> Result<Book, FindBookError> {
        books::table
            .filter(books::id.eq(id))
            .get_result::<db::Book>(dbconn)
            .optional()?
            .ok_or(FindBookError::NotFound)
            .map(|data| Book { data })
    }
}

#[derive(Debug, Fail)]
pub enum FindBookError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// No module found matching given criteria.
    #[fail(display = "No such module")]
    NotFound,
}

impl_from! { for FindBookError ;
    DbError => |e| FindBookError::Database(e),
}
