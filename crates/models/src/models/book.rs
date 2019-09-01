use diesel::{prelude::*, result::Error as DbError};
use serde::Serialize;
use uuid::Uuid;

use crate::{audit, db::{Connection, models as db, schema::books}};
use super::{AssertExists, BookPart, FindModelResult, Model};

/// A book is a collection of modules and their structure.
#[derive(Debug)]
pub struct Book {
    data: db::Book,
}

/// A subset of book's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    id: Uuid,
    title: String,
}

impl Model for Book {
    const ERROR_CATEGORY: &'static str = "book";

    type Id = Uuid;
    type Database = db::Book;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, id: Uuid) -> FindModelResult<Self> {
        books::table
            .filter(books::id.eq(id))
            .get_result::<db::Book>(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db(data: db::Book) -> Self {
        Book { data }
    }

    fn into_db(self) -> db::Book {
        self.data
    }

    fn id(&self) -> Uuid {
        self.data.id
    }

    fn get_public(&self) -> Self::Public {
        Public {
            id: self.data.id,
            title: self.data.title.clone(),
        }
    }
}

impl Book {
    /// Get all books.
    ///
    /// *Warning*: this function is temporary and will be removed once we figure
    /// out how to do pagination.
    pub fn all(db: &Connection) -> Result<Vec<Book>, DbError> {
        books::table
            .get_results::<db::Book>(db)
            .map(|v| v.into_iter().map(|data| Book { data }).collect())
    }

    /// Create a new book.
    pub fn create(db: &Connection, title: &str) -> Result<Book, DbError> {
        let data = diesel::insert_into(books::table)
            .values(db::NewBook {
                id: Uuid::new_v4(),
                title,
            })
            .get_result::<db::Book>(db)?;

        audit::log_db(db, "books", data.id, "create", LogCreation { title });

        Ok(Book { data })
    }

    /// Delete this book.
    ///
    /// This will delete only the book and its structure. Modules added to this
    /// book will not be affected.
    pub fn delete(self, db: &Connection) -> Result<(), DbError> {
        diesel::delete(&self.data).execute(db)?;
        audit::log_db(db, "books", self.data.id, "delete", ());
        Ok(())
    }

    /// Load root part of this book.
    pub fn root_part(&self, db: &Connection) -> Result<BookPart, DbError> {
        BookPart::by_id(db, (self.data.id, 0)).assert_exists()
    }

    /// Change title of this book.
    pub fn set_title(&mut self, db: &Connection, title: String) -> Result<(), DbError> {
        diesel::update(&self.data)
            .set(books::title.eq(&title))
            .execute(db)?;

        audit::log_db(db, "books", self.data.id, "set-title", &title);

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

#[derive(Serialize)]
struct LogCreation<'a> {
    title: &'a str,
}
