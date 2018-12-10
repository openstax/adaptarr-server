use actix_web::{HttpResponse, ResponseError};
use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};
use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::book_parts,
};
use super::Module;

/// Part of a book.
///
/// Structure and contents of a book are defined by parts, which may be either
/// modules, or a group; an ordered collection of other parts.
#[derive(Debug)]
pub struct BookPart {
    data: db::BookPart,
}

/// A subset of book part's data that can safely be publicly exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    number: i32,
    title: String,
    #[serde(flatten)]
    part: Variant<i32>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Variant<Part> {
    Module {
        id: Uuid,
    },
    Group {
        parts: Vec<Part>,
    },
}

#[derive(Debug, Serialize)]
pub struct Tree {
    number: i32,
    title: String,
    #[serde(flatten)]
    part: Variant<Tree>,
}

impl BookPart {
    /// Find part of a book by ID.
    pub fn by_id(dbconn: &Connection, book: Uuid, id: i32)
    -> Result<BookPart, FindBookPartError> {
        book_parts::table
            .filter(book_parts::book.eq(book).and(book_parts::id.eq(id)))
            .get_result::<db::BookPart>(dbconn)
            .optional()?
            .ok_or(FindBookPartError::NotFound)
            .map(|data| BookPart { data })
    }

    /// Delete this book part.
    ///
    /// This method cannot be used to delete group 0 (the main group of a book).
    /// To delete group 0 use [`Book::delete()`] instead.
    pub fn delete(self, dbconn: &Connection) -> Result<(), DeletePartError> {
        if self.data.id == 0 {
            return Err(DeletePartError::RootGroup);
        }

        diesel::delete(&self.data).execute(dbconn)?;
        Ok(())
    }

    /// Get parts of this group.
    pub fn get_parts(&self, dbconn: &Connection) -> Result<Vec<i32>, GetPartsError> {
        if self.data.module.is_some() {
            return Err(GetPartsError::IsAModule);
        }

        book_parts::table
            .filter(book_parts::book.eq(self.data.book)
                .and(book_parts::parent.eq(self.data.id))
                .and(book_parts::id.ne(self.data.id)))
            .order_by(book_parts::index.asc())
            .get_results::<db::BookPart>(dbconn)
            .map_err(Into::into)
            .map(|r| r.into_iter().map(|p| p.id).collect())
    }

    /// Get the public portion of this book part's data.
    pub fn get_public(&self, dbconn: &Connection) -> Result<PublicData, DbError> {
        Ok(PublicData {
            number: self.data.id,
            title: self.data.title.clone(),
            part: if let Some(id) = self.data.module {
                Variant::Module { id }
            } else {
                Variant::Group {
                    parts: self.get_parts(dbconn)
                        .map_err(|e| match e {
                            GetPartsError::Database(e) => e,
                            GetPartsError::IsAModule => unreachable!(),
                        })?,
                }
            },
        })
    }

    /// Get contents of this group as a tree.
    pub fn get_tree(&self, dbconn: &Connection) -> Result<Tree, DbError> {
        let part = if let Some(id) = self.data.module {
            Variant::Module { id }
        } else {
            let parts = book_parts::table
                .filter(book_parts::book.eq(self.data.book)
                    .and(book_parts::parent.eq(self.data.id))
                    .and(book_parts::id.ne(self.data.id)))
                .order_by(book_parts::index.asc())
                .get_results::<db::BookPart>(dbconn)?
                .into_iter()
                .map(|data| BookPart { data }.get_tree(dbconn))
                .collect::<Result<Vec<_>, _>>()?;

            Variant::Group { parts }
        };

        Ok(Tree {
            number: self.data.id,
            title: self.data.title.clone(),
            part,
        })
    }

    /// Insert a module at index.
    pub fn insert_module<T>(
        &self,
        dbconn: &Connection,
        index: i32,
        title: T,
        module: &Module,
    ) -> Result<BookPart, CreatePartError>
    where
        T: AsRef<str>,
    {
        self.create_at(dbconn, index, title.as_ref(), Some(module))
    }

    /// Create a new group at index.
    pub fn create_group<T>(&self, dbconn: &Connection, index: i32, title: T)
    -> Result<BookPart, CreatePartError>
    where
        T: AsRef<str>,
    {
        self.create_at(dbconn, index, title.as_ref(), None)
    }

    /// Create a new book part at an index within this book part.
    fn create_at(
        &self,
        dbconn: &Connection,
        index: i32,
        title: &str,
        module: Option<&Module>,
    ) -> Result<BookPart, CreatePartError> {
        if self.data.module.is_some() {
            return Err(CreatePartError::IsAModule);
        }

        dbconn.transaction(|| {
            let parts = book_parts::table
                .filter(book_parts::book.eq(self.data.book)
                    .and(book_parts::parent.eq(self.data.id))
                    .and(book_parts::index.ge(index)));
            diesel::update(parts)
                .set(book_parts::index.eq(book_parts::index + 1))
                .execute(dbconn)?;

            diesel::insert_into(book_parts::table)
                .values(&db::NewBookPart {
                    book: self.data.book,
                    title: title,
                    module: module.map(|m| m.id()),
                    parent: self.data.id,
                    index,
                })
                .get_result::<db::BookPart>(dbconn)
                .map_err(Into::into)
                .map(|data| BookPart { data })
        })
    }
}

impl std::ops::Deref for BookPart {
    type Target = db::BookPart;

    fn deref(&self) -> &db::BookPart {
        &self.data
    }
}

#[derive(Debug, Fail)]
pub enum FindBookPartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// No module found matching given criteria.
    #[fail(display = "No such module")]
    NotFound,
}

impl_from! { for FindBookPartError ;
    DbError => |e| FindBookPartError::Database(e),
}

impl ResponseError for FindBookPartError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            FindBookPartError::Database(_) =>
                HttpResponse::InternalServerError().finish(),
            FindBookPartError::NotFound =>
                HttpResponse::NotFound().finish(),
        }
    }
}

#[derive(Debug, Fail)]
pub enum DeletePartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// Deleting group 0 is not possible.
    #[fail(display = "Cannot delete group 0")]
    RootGroup,
}

impl_from! { for DeletePartError ;
    DbError => |e| DeletePartError::Database(e),
}

#[derive(Debug, Fail)]
pub enum GetPartsError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// This part is a module, it has no parts of its own.
    #[fail(display = "Module has no parts")]
    IsAModule,
}

impl_from! { for GetPartsError ;
    DbError => |e| GetPartsError::Database(e),
}

#[derive(Debug, Fail)]
pub enum CreatePartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// This part is a module, it has no parts of its own.
    #[fail(display = "Module has no parts")]
    IsAModule,
}

impl_from! { for CreatePartError ;
    DbError => |e| CreatePartError::Database(e),
}
