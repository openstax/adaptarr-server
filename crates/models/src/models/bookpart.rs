use adaptarr_error::ApiError;
use adaptarr_macros::From;
use diesel::{Connection as _, prelude::*, result::Error as DbError};
use failure::Fail;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{audit, db::{Connection, models as db, schema::book_parts}};
use super::{AssertExists, FindModelError, FindModelResult, Model, Module};

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
pub struct Public {
    number: i32,
    title: String,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    part: Option<Variant<i32>>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Variant<Part> {
    Module(<Module as Model>::Public),
    Group {
        parts: Vec<Part>,
    },
}

#[derive(Debug, Serialize)]
pub struct Tree {
    pub number: i32,
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

impl Model for BookPart {
    const ERROR_CATEGORY: &'static str = "book:part";

    type Id = (Uuid, i32);
    type Database = db::BookPart;
    type Public = Public;
    type PublicParams = ();

    fn by_id(db: &Connection, (book, id): (Uuid, i32))
    -> FindModelResult<BookPart> {
        book_parts::table
            .filter(book_parts::book.eq(book).and(book_parts::id.eq(id)))
            .get_result::<db::BookPart>(db)
            .map(Self::from_db)
            .map_err(From::from)
    }

    fn from_db(data: db::BookPart) -> Self {
        BookPart { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        (self.data.book, self.data.id)
    }

    fn get_public(&self) -> Self::Public {
        Public {
            number: self.data.id,
            title: self.data.title.clone(),
            part: None,
        }
    }

    fn get_public_full(&self, db: &Connection, _: ()) -> Result<Public, DbError> {
        Ok(Public {
            number: self.data.id,
            title: self.data.title.clone(),
            part: Some(if let Some(id) = self.data.module {
                Module::by_id(db, id).assert_exists()?
                    .get_public_full(db, ())
                    .map(Variant::Module)?
            } else {
                Variant::Group {
                    parts: self.get_parts(db)
                        .map_err(|e| match e {
                            GetPartsError::Database(e) => e,
                            GetPartsError::IsAModule => unreachable!(),
                        })?,
                }
            }),
        })
    }
}

impl BookPart {
    /// Delete this book part.
    ///
    /// This method cannot be used to delete group 0 (the main group of a book).
    /// To delete group 0 use [`Book::delete()`] instead.
    pub fn delete(self, db: &Connection) -> Result<(), DeletePartError> {
        if self.data.id == 0 {
            return Err(DeletePartError::RootGroup);
        }

        diesel::delete(&self.data).execute(db)?;

        audit::log_db(
            db, "books", self.data.book, "delete-part", self.data.id);

        Ok(())
    }

    /// Get parts of this group.
    pub fn get_parts(&self, db: &Connection) -> Result<Vec<i32>, GetPartsError> {
        if self.data.module.is_some() {
            return Err(GetPartsError::IsAModule);
        }

        book_parts::table
            .filter(book_parts::book.eq(self.data.book)
                .and(book_parts::parent.eq(self.data.id))
                .and(book_parts::id.ne(self.data.id)))
            .order_by(book_parts::index.asc())
            .get_results::<db::BookPart>(db)
            .map_err(Into::into)
            .map(|r| r.into_iter().map(|p| p.id).collect())
    }

    /// Get contents of this group as a tree.
    pub fn get_tree(&self, db: &Connection) -> Result<Tree, DbError> {
        let part = if let Some(id) = self.data.module {
            Module::by_id(db, id)
                .assert_exists()?
                .get_public_full(db, ())
                .map(Variant::Module)?
        } else {
            let parts = book_parts::table
                .filter(book_parts::book.eq(self.data.book)
                    .and(book_parts::parent.eq(self.data.id))
                    .and(book_parts::id.ne(self.data.id)))
                .order_by(book_parts::index.asc())
                .get_results::<db::BookPart>(db)?
                .into_iter()
                .map(|data| BookPart { data }.get_tree(db))
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
    pub fn clear(&mut self, db: &Connection) -> Result<(), DbError> {
        diesel::delete(book_parts::table
            .filter(book_parts::book.eq(self.data.book)
                .and(book_parts::parent.eq(self.data.id))
                .and(book_parts::id.ne(0))))
            .execute(db)?;

        audit::log_db(db, "books", self.data.book, "clear-part", self.data.id);

        Ok(())
    }

    /// Insert a module at index.
    pub fn insert_module<T>(
        &self,
        db: &Connection,
        index: i32,
        title: T,
        module: &Module,
    ) -> Result<BookPart, CreatePartError>
    where
        T: AsRef<str>,
    {
        self.create_at(db, index, title.as_ref(), Some(module))
    }

    /// Create a new group at index.
    pub fn create_group<T>(&self, db: &Connection, index: i32, title: T)
    -> Result<BookPart, CreatePartError>
    where
        T: AsRef<str>,
    {
        self.create_at(db, index, title.as_ref(), None)
    }

    /// Create a new book part at an index within this book part.
    fn create_at(
        &self,
        db: &Connection,
        index: i32,
        title: &str,
        module: Option<&Module>,
    ) -> Result<BookPart, CreatePartError> {
        if self.data.module.is_some() {
            return Err(CreatePartError::IsAModule);
        }

        db.transaction(|| {
            let parts = book_parts::table
                .filter(book_parts::book.eq(self.data.book)
                    .and(book_parts::parent.eq(self.data.id))
                    .and(book_parts::index.ge(index)));
            diesel::update(parts)
                .set(book_parts::index.eq(book_parts::index + 1))
                .execute(db)?;

            let data = diesel::insert_into(book_parts::table)
                .values(&db::NewBookPart {
                    book: self.data.book,
                    title,
                    module: module.map(Module::id),
                    parent: self.data.id,
                    index,
                })
                .get_result::<db::BookPart>(db)?;

            audit::log_db(db, "books", self.data.book, "create-part",
                LogCreate {
                    part: data.id,
                    parent: self.data.id,
                    index,
                    title,
                    module: module.map(|m| m.id()),
                });

            Ok(BookPart { data })
        })
    }

    /// Recursively create a tree of book parts.
    pub fn create_tree(&self, db: &Connection, index: i32, tree: NewTree)
    -> Result<Tree, RealizeTemplateError> {
        if self.data.module.is_some() {
            return Err(RealizeTemplateError::IsAModule);
        }

        db.transaction(|| {
            self.create_part_inner(db, index, tree)
        })
    }

    fn create_part_inner(&self, db: &Connection, index: i32, tree: NewTree)
    -> Result<Tree, RealizeTemplateError> {
        match tree {
            NewTree::Module { title, module } => {
                let module = Module::by_id(db, module)?;

                let part = self.insert_module(
                    db,
                    index,
                    title.as_ref().map_or(module.title.as_str(), String::as_str),
                    &module,
                )?;

                Ok(Tree {
                    number: part.id,
                    title: title.unwrap_or_else(|| module.title.clone()),
                    part: Variant::Module(module.get_public_full(db, ())?),
                })
            }
            NewTree::Group { title, parts } => {
                let group = self.create_group(db, index, title.as_str())?;

                Ok(Tree {
                    number: group.id,
                    title,
                    part: Variant::Group {
                        parts: parts.into_iter()
                            .enumerate()
                            .map(|(index, part)| group.create_part_inner(
                                db, index as i32, part))
                            .collect::<Result<Vec<_>, _>>()?,
                    },
                })
            }
        }
    }

    /// Change title of this book part.
    pub fn set_title(&mut self, db: &Connection, title: &str) -> Result<(), DbError> {
        diesel::update(&self.data)
            .set(book_parts::title.eq(title))
            .execute(db)?;

        audit::log_db(db, "books", self.data.book, "set-part-title",
            LogSetTitle { part: self.data.id, title });

        self.data.title = title.to_owned();

        Ok(())
    }

    /// Move this part to another group, or another location in the same group.
    pub fn reparent(&mut self, db: &Connection, new: &BookPart, index: i32)
    -> Result<(), ReparentPartError> {
        if new.module.is_some() {
            return Err(ReparentPartError::IsAModule);
        }

        self.data = db.transaction::<_, DbError, _>(|| {
            // First make place in the new parent.
            let siblings = book_parts::table.filter(
                book_parts::book.eq(new.book)
                    .and(book_parts::parent.eq(new.id))
                    .and(book_parts::index.ge(index))
            );
            diesel::update(siblings)
                .set(book_parts::index.eq(book_parts::index + 1))
                .execute(db)?;

            // Now we can move self to new without violating
            // unique (book, parent, index).
            let data = diesel::update(&self.data)
                .set(&db::NewBookPartLocation {
                    book: new.book,
                    parent: new.id,
                    index,
                })
                .get_result::<db::BookPart>(db)?;

            // Now we can shift old sibling back by one to fill the gap, without
            // violating unique (book, parent, index) on the old parent.
            let siblings = book_parts::table.filter(
                book_parts::book.eq(self.data.book)
                    .and(book_parts::parent.eq(self.data.parent))
                    .and(book_parts::index.gt(self.data.index))
            );
            diesel::update(siblings)
                .set(book_parts::index.eq(book_parts::index - 1))
                .execute(db)?;

            audit::log_db(db, "books", self.data.book, "reparent-part",
                LogReparent {
                    part: self.data.id,
                    parent: new.id,
                    index,
                });

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

#[derive(ApiError, Debug, Fail, From)]
pub enum CreatePartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// This part is a module, it has no parts of its own.
    #[fail(display = "Module has no parts")]
    #[api(code = "bookpart:create-part:is-module", status = "BAD_REQUEST")]
    IsAModule,
}

#[derive(ApiError, Debug, Fail, From)]
pub enum DeletePartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// Deleting group 0 is not possible.
    #[fail(display = "Cannot delete group 0")]
    #[api(code = "bookpart:delete:is-root", status = "BAD_REQUEST")]
    RootGroup,
}

#[derive(ApiError, Debug, Fail, From)]
pub enum GetPartsError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// This part is a module, it has no parts of its own.
    #[fail(display = "Module has no parts")]
    #[api(code = "bookpart:get-parts:is-module", status = "BAD_REQUEST")]
    IsAModule,
}

#[derive(ApiError, Debug, Fail, From)]
pub enum ReparentPartError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// New parent is a module, it has no parts of its own.
    #[fail(display = "Parent cannot be a module")]
    #[api(code = "bookpart:reparent:is-module", status = "BAD_REQUEST")]
    IsAModule,
}


#[derive(ApiError, Debug, Fail, From)]
pub enum RealizeTemplateError {
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] #[from] DbError),
    /// This part is a module, it has no parts of its own.
    #[fail(display = "Module has no parts")]
    #[api(code = "bookpart:create-part:is-module", status = "BAD_REQUEST")]
    IsAModule,
    #[fail(display = "Module not found: {}", _0)]
    ModuleNotFound(#[cause] #[from] FindModelError<Module>),
    #[fail(display = "Part could not be created: {}", _0)]
    CreatePart(#[cause] CreatePartError),
}

impl From<CreatePartError> for RealizeTemplateError {
    fn from(e: CreatePartError) -> Self {
        match e {
            CreatePartError::Database(e) => RealizeTemplateError::Database(e),
            _ => RealizeTemplateError::CreatePart(e),
        }
    }
}

#[derive(Serialize)]
struct LogSetTitle<'a> {
    part: i32,
    title: &'a str,
}

#[derive(Serialize)]
struct LogCreate<'a> {
    part: i32,
    parent: i32,
    index: i32,
    title: &'a str,
    module: Option<Uuid>,
}

#[derive(Serialize)]
struct LogReparent {
    part: i32,
    parent: i32,
    index: i32,
}
