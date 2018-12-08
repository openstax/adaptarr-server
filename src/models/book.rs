use actix_web::{HttpResponse, ResponseError};
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

/// A subset of book's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    id: Uuid,
    title: String,
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

    /// Create a new book.
    pub fn create(dbconn: &Connection, title: &str) -> Result<Book, DbError> {
        diesel::insert_into(books::table)
            .values(db::NewBook {
                id: Uuid::new_v4(),
                title,
            })
            .get_result::<db::Book>(dbconn)
            .map(|data| Book { data })
    }

    /// Delete this book.
    ///
    /// This will delete only the book and its structure. Modules added to this
    /// book will not be affected.
    pub fn delete(self, dbconn: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(dbconn)?;
        Ok(())
    }

    /// Get the public portion of this book's data.
    pub fn get_public(&self) -> PublicData {
        PublicData {
            id: self.data.id,
            title: self.data.title.clone(),
        }
    }

    /// Change title of this book.
    pub fn set_title(&mut self, dbconn: &Connection, title: String) -> Result<(), DbError> {
        diesel::update(&self.data)
            .set(books::title.eq(&title))
            .execute(dbconn)?;
        self.data.title = title;
        Ok(())
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

impl ResponseError for FindBookError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            FindBookError::Database(_) =>
                HttpResponse::InternalServerError().finish(),
            FindBookError::NotFound =>
                HttpResponse::NotFound().finish(),
        }
    }
}
