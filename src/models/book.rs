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
use super::bookpart::{BookPart, FindBookPartError};

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
    /// Get all books.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all(dbconn: &Connection) -> Result<Vec<Book>, DbError> {
        books::table
            .get_results::<db::Book>(dbconn)
            .map(|v| v.into_iter().map(|data| Book { data }).collect())
    }

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

    /// Load root part of this book.
    pub fn root_part(&self, dbconn: &Connection) -> Result<BookPart, DbError> {
        BookPart::by_id(dbconn, self.data.id, 0)
            .map_err(|e| match e {
                FindBookPartError::Database(e) => e,
                FindBookPartError::NotFound => panic!(
                    "Inconsistent database: no root part for book {}",
                    self.data.id,
                ),
            })
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

impl std::ops::Deref for Book {
    type Target = db::Book;

    fn deref(&self) -> &db::Book {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindBookError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// No book found matching given criteria.
    #[fail(display = "No such book")]
    #[api(code = "book:not-found", status = "NOT_FOUND")]
    NotFound,
}

impl_from! { for FindBookError ;
    DbError => |e| FindBookError::Database(e),
}
