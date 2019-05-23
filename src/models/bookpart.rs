use diesel::{
    Connection as _Connection,
    prelude::*,
    result::Error as DbError,
};
use failure::Fail;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ApiError,
    db::{
        Connection,
        models as db,
        schema::book_parts,
    },
};
use super::module::{Module, FindModuleError};

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

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum NewTree {
    Module {
        title: Option<String>,
        module: Uuid,
    },
    Group {
        title: String,
        #[serde(default)]
        parts: Vec<NewTree>,
    },
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

    /// Clear this part.
    ///
    /// This function does nothing if this part is a module.
    pub fn clear(&mut self, dbconn: &Connection) -> Result<(), DbError> {
        diesel::delete(book_parts::table
            .filter(book_parts::book.eq(self.data.book)
                .and(book_parts::parent.eq(self.data.id))
                .and(book_parts::id.ne(0))))
            .execute(dbconn)?;
        Ok(())
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
                    title,
                    module: module.map(Module::id),
                    parent: self.data.id,
                    index,
                })
                .get_result::<db::BookPart>(dbconn)
                .map_err(Into::into)
                .map(|data| BookPart { data })
        })
    }

    /// Recursively create a tree of book parts.
    pub fn create_tree(&self, dbconn: &Connection, index: i32, tree: NewTree)
    -> Result<Tree, RealizeTemplateError> {
        if self.data.module.is_some() {
            return Err(RealizeTemplateError::IsAModule);
        }

        dbconn.transaction(|| {
            self.create_part_inner(dbconn, index, tree)
        })
    }

    fn create_part_inner(&self, dbconn: &Connection, index: i32, tree: NewTree)
    -> Result<Tree, RealizeTemplateError> {
        match tree {
            NewTree::Module { title, module } => {
                let module = Module::by_id(dbconn, module)?;

                let part = self.insert_module(
                    dbconn,
                    index,
                    title.as_ref().map_or(module.title.as_str(), String::as_str),
                    &module,
                )?;

                Ok(Tree {
                    number: part.id,
                    title: title.unwrap_or_else(|| module.title.clone()),
                    part: Variant::Module {
                        id: module.id(),
                    },
                })
            }
            NewTree::Group { title, parts } => {
                let group = self.create_group(dbconn, index, title.as_str())?;

                Ok(Tree {
                    number: group.id,
                    title,
                    part: Variant::Group {
                        parts: parts.into_iter()
                            .enumerate()
                            .map(|(index, part)| group.create_part_inner(
                                dbconn, index as i32, part))
                            .collect::<Result<Vec<_>, _>>()?,
                    },
                })
            }
        }
    }

    /// Change title of this book part.
    pub fn set_title(&mut self, dbconn: &Connection, title: &str) -> Result<(), DbError> {
        diesel::update(&self.data)
            .set(book_parts::title.eq(title))
            .execute(dbconn)?;

        self.data.title = title.to_owned();

        Ok(())
    }

    /// Move this part to another group, or another location in the same group.
    pub fn reparent(&mut self, dbconn: &Connection, new: &BookPart, index: i32)
    -> Result<(), ReparentPartError> {
        if new.module.is_some() {
            return Err(ReparentPartError::IsAModule);
        }

        self.data = dbconn.transaction::<_, DbError, _>(|| {
            // First make place in the new parent.
            let siblings = book_parts::table.filter(
                book_parts::book.eq(new.book)
                    .and(book_parts::parent.eq(new.id))
                    .and(book_parts::index.ge(index))
            );
            diesel::update(siblings)
                .set(book_parts::index.eq(book_parts::index + 1))
                .execute(dbconn)?;

            // Now we can move self to new without violating
            // unique (book, parent, index).
            let data = diesel::update(&self.data)
                .set(&db::NewBookPartLocation {
                    book: new.book,
                    parent: new.id,
                    index,
                })
                .get_result::<db::BookPart>(dbconn)?;

            // Now we can shift old sibling back by one to fill the gap, without
            // violating unique (book, parent, index) on the old parent.
            let siblings = book_parts::table.filter(
                book_parts::book.eq(self.data.book)
                    .and(book_parts::parent.eq(self.data.parent))
                    .and(book_parts::index.gt(self.data.index))
            );
            diesel::update(siblings)
                .set(book_parts::index.eq(book_parts::index - 1))
                .execute(dbconn)?;

            Ok(data)
        })?;

        Ok(())
    }
}

impl std::ops::Deref for BookPart {
    type Target = db::BookPart;

    fn deref(&self) -> &db::BookPart {
        &self.data
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum FindBookPartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// No module found matching given criteria.
    #[fail(display = "No such module")]
    #[api(internal)]
    NotFound,
}

impl_from! { for FindBookPartError ;
    DbError => |e| FindBookPartError::Database(e),
}

#[derive(ApiError, Debug, Fail)]
pub enum DeletePartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// Deleting group 0 is not possible.
    #[fail(display = "Cannot delete group 0")]
    #[api(code = "bookpart:delete:is-root", status = "BAD_REQUEST")]
    RootGroup,
}

impl_from! { for DeletePartError ;
    DbError => |e| DeletePartError::Database(e),
}

#[derive(ApiError, Debug, Fail)]
pub enum GetPartsError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// This part is a module, it has no parts of its own.
    #[fail(display = "Module has no parts")]
    #[api(code = "bookpart:get-parts:is-module", status = "BAD_REQUEST")]
    IsAModule,
}

impl_from! { for GetPartsError ;
    DbError => |e| GetPartsError::Database(e),
}

#[derive(ApiError, Debug, Fail)]
pub enum CreatePartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// This part is a module, it has no parts of its own.
    #[fail(display = "Module has no parts")]
    #[api(code = "bookpart:create-part:is-module", status = "BAD_REQUEST")]
    IsAModule,
}

impl_from! { for CreatePartError ;
    DbError => |e| CreatePartError::Database(e),
}

#[derive(ApiError, Debug, Fail)]
pub enum RealizeTemplateError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// This part is a module, it has no parts of its own.
    #[fail(display = "Module has no parts")]
    #[api(code = "bookpart:create-part:is-module", status = "BAD_REQUEST")]
    IsAModule,
    #[fail(display = "Module not found: {}", _0)]
    ModuleNotFound(#[cause] FindModuleError),
    #[fail(display = "Part could not be created: {}", _0)]
    CreatePart(#[cause] CreatePartError),
}

impl_from! { for RealizeTemplateError ;
    DbError => |e| RealizeTemplateError::Database(e),
    FindModuleError => |e| RealizeTemplateError::ModuleNotFound(e),
    CreatePartError => |e| match e {
        CreatePartError::Database(e) => RealizeTemplateError::Database(e),
        _ => RealizeTemplateError::CreatePart(e),
    },
}

#[derive(ApiError, Debug, Fail)]
pub enum ReparentPartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// New parent is a module, it has no parts of its own.
    #[fail(display = "Parent cannot be a module")]
    #[api(code = "bookpart:reparent:is-module", status = "BAD_REQUEST")]
    IsAModule,
}

impl_from! { for ReparentPartError ;
    DbError => |e| ReparentPartError::Database(e),
}
