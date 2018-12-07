use actix_web::{HttpResponse, ResponseError};
use diesel::{
    prelude::*,
    result::Error as DbError,
};
use uuid::Uuid;

use crate::db::{
    Connection,
    models as db,
    schema::book_parts,
};

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
