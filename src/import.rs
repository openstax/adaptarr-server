//! File upload and importing ZIPs of modules and collections.

use actix::{Actor, Addr, Handler, SyncArbiter, SyncContext, Message};
use actix_web::{HttpResponse, ResponseError};
use diesel::result::Error as DbError;
use minidom::Element as XmlElement;
use std::{path::PathBuf, io::Read, str::FromStr};
use tempfile::NamedTempFile;
use zip::{ZipArchive, result::ZipError};

use crate::{
    config::Storage,
    db::Pool,
    models::{
        file::{File, CreateFileError},
        module::{Module, ReplaceModuleError},
    },
};

/// Request a new module to be created from contents of a ZIP file
pub struct ImportModule {
    pub title: String,
    pub file: NamedTempFile,
}

impl Message for ImportModule {
    type Result = Result<Module, ImportError>;
}

/// Requests contents of an existing module to be replaced with contents of
/// a ZIP file.
pub struct ReplaceModule {
    pub module: Module,
    pub file: NamedTempFile,
}

impl Message for ReplaceModule {
    type Result = Result<Module, ImportError>;
}

/// Actix actor processing ZIPs in a background worker.
pub struct Importer {
    pool: Pool,
    config: Storage,
}

impl Importer {
    pub fn new(pool: Pool, config: Storage) -> Importer {
        Importer {
            pool,
            config,
        }
    }

    pub fn start(pool: Pool, config: Storage) -> Addr<Importer> {
        SyncArbiter::start(1, move || Importer::new(pool.clone(), config.clone()))
    }

    /// Process a zipped module and extract index.cnxml and other media files
    /// from it.
    fn process_module_zip(&mut self, mut file: NamedTempFile)
    -> Result<(File, Vec<(String, File)>), ImportError> {
        let mut zip = ZipArchive::new(file.as_file_mut())?;

        // NOTE: Looking for index imperatively because rustc doesn't seem able
        // to infer types for iterator combinators (“consider giving this
        // closure parameter a type” on a parameter that already has a type).
        let mut index = None;

        for inx in 0..zip.len() {
            let file = zip.by_index(inx)?;
            let path = file.sanitized_name();
            let name = path
                .file_name()
                .unwrap();

            if name == "index.cnxml" {
                index = Some((
                    inx,
                    path.parent().map_or_else(|| PathBuf::new(), |p| p.to_owned()),
                ));
                break;
            }
        }

        let (index, base_path) = index.ok_or(ImportError::IndexMissing)?;

        let db = self.pool.get()?;

        let index_file = File::from_read(
            &*db,
            &self.config,
            zip.by_index(index)?,
        )?;

        let mut files = Vec::new();

        for inx in 0..zip.len() {
            // Don't import index twice.
            if inx == index {
                continue;
            }

            let file = zip.by_index(inx)?;

            // Don't import directories.
            if file.size() == 0 {
                continue;
            }

            let path = file.sanitized_name();

            if !path.starts_with(&base_path) {
                continue;
            }

            let name = path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();

            let file = File::from_read(&*db, &self.config, file)?;

            files.push((name, file));
        }

        Ok((index_file, files))
    }

    /// Create a new module from a ZIP of its contents.
    fn create_module(&mut self, title: String, file: NamedTempFile)
    -> Result<Module, ImportError> {
        let (index, files) = self.process_module_zip(file)?;
        let db = self.pool.get()?;
        let module = Module::create(&*db, &title, index, files)?;

        Ok(module)
    }

    /// Import a zipped module onto an existing one.
    fn replace_module(&mut self, mut module: Module, file: NamedTempFile)
    -> Result<Module, ImportError> {
        let (index, files) = self.process_module_zip(file)?;

        let db = self.pool.get()?;
        module.replace(&*db, index, files)?;

        Ok(module)
    }

    /// Process a zipped collection and extract from it collection.xml, list
    /// of modules, and their structure.
    fn preprocess_collection_zip<'c, 'f>(&'c mut self, file: &'f mut NamedTempFile)
    -> Result<(ZipArchive<&'f mut std::fs::File>, Collection, PathBuf), ImportError> {
        let mut zip = ZipArchive::new(file.as_file_mut())?;

        // NOTE: Looking for collection.xml imperatively because rustc doesn't
        // seem able to infer types for iterator combinators (“consider giving
        // this closure parameter a type” on a parameter that already has
        // a type).
        let mut colxml = None;

        for inx in 0..zip.len() {
            let file = zip.by_index(inx)?;
            let path = file.sanitized_name();
            let name = path
                .file_name()
                .unwrap();

            if name == "collection.xml" {
                println!("collection.xml found at {}", file.name());
                colxml = Some((
                    inx,
                    path.parent().map_or_else(|| PathBuf::new(), |p| p.to_owned()),
                ));
                break;
            }
        }

        let (colxml_inx, base_path) = colxml.ok_or(ImportError::ColxmlMissing)?;

        let mut coldata = String::new();
        zip.by_index(colxml_inx)?.read_to_string(&mut coldata)?;
        let coldata = XmlElement::from_str(&coldata)?;
        let coldata = Collection::from_xml(&coldata)?;

        Ok((zip, coldata, base_path))
    }
}

impl Actor for Importer {
    type Context = SyncContext<Self>;
}

impl Handler<ImportModule> for Importer {
    type Result = Result<Module, ImportError>;

    fn handle(&mut self, msg: ImportModule, _: &mut Self::Context) -> Self::Result {
        let ImportModule { title, file } = msg;

        self.create_module(title, file)
    }
}

impl Handler<ReplaceModule> for Importer {
    type Result = Result<Module, ImportError>;

    fn handle(&mut self, msg: ReplaceModule, _: &mut Self::Context) -> Self::Result {
        let ReplaceModule { module, file } = msg;

        self.replace_module(module, file)
    }
}

#[derive(Debug, Fail)]
pub enum ImportError {
    /// There was a problem with the ZIP archive.
    #[fail(display = "{}", _0)]
    Archive(#[cause] ZipError),
    /// There was no file named index.cnxml in the ZIP archive.
    #[fail(display = "Archive is missing index.cnxml")]
    IndexMissing,
    /// There was no file named collection.xml in the ZIP archive.
    #[fail(display = "Archive is missing collection.xml")]
    ColxmlMissing,
    /// There was a problem obtaining database connection.
    #[fail(display = "Cannot obtain database connection: {}", _0)]
    DbPool(#[cause] r2d2::Error),
    /// A file could not be created.
    #[fail(display = "Cannot create file: {}", _0)]
    FileCreation(#[cause] CreateFileError),
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    Database(#[cause] DbError),
    /// Replacing module's contents failed.
    #[fail(display = "{}", _0)]
    ReplaceModule(#[cause] ReplaceModuleError),
    /// One of the XML files was invalid.
    #[fail(display = "Invalid XML: {}", _0)]
    InvalidXml(#[cause] minidom::Error),
    /// collection.xml did not conform to schema.
    #[fail(display = "Invalid collection.xml: {}", _0)]
    MalformedXml(#[cause] ParseCollectionError),
    /// An operating system error.
    #[fail(display = "System error: {}", _0)]
    System(#[cause] std::io::Error),
}

impl_from! { for ImportError ;
    r2d2::Error => |e| ImportError::DbPool(e),
    ZipError => |e| ImportError::Archive(e),
    CreateFileError => |e| ImportError::FileCreation(e),
    DbError => |e| ImportError::Database(e),
    ReplaceModuleError => |e| ImportError::ReplaceModule(e),
    minidom::Error => |e| ImportError::InvalidXml(e),
    ParseCollectionError => |e| ImportError::MalformedXml(e),
    std::io::Error => |e| ImportError::System(e),
}

impl ResponseError for ImportError {
    fn error_response(&self) -> HttpResponse {
        use self::ImportError::*;

        match *self {
            Archive(_) | IndexMissing | ColxmlMissing | InvalidXml(_)
            | MalformedXml(_) =>
                HttpResponse::BadRequest().body(self.to_string()),
            DbPool(_) | Database(_) | System(_) =>
                HttpResponse::InternalServerError().finish(),
            FileCreation(ref e) => e.error_response(),
            ReplaceModule(ref e) => e.error_response(),
        }
    }
}

#[derive(Debug)]
struct Collection {
    title: String,
    content: Vec<Element>,
}

#[derive(Debug)]
enum Element {
    Module(ModuleElement),
    Subcollection(Subcollection),
}

#[derive(Debug)]
struct ModuleElement {
    title: String,
    document: String,
}

#[derive(Debug)]
struct Subcollection {
    title: String,
    content: Vec<Element>,
}

#[derive(Debug, Fail)]
pub enum ParseCollectionError {
    #[fail(display = "Missing required element {} from namespace {}", _1, _0)]
    MissingElement(&'static str, &'static str),
    #[fail(display = "Element {} from namespace {} was not expected", _1, _0)]
    InvalidElement(String, String),
    #[fail(
        display = "Missing required attribute {} of element {} from namespace {}",
        _0, _2, _1,
    )]
    MissingAttribute(&'static str, &'static str, &'static str),
}

const COL_NS: &str = "http://cnx.rice.edu/collxml";
const MDML_NS: &str = "http://cnx.rice.edu/mdml";

impl Collection {
    fn from_xml(e: &XmlElement) -> Result<Collection, ParseCollectionError> {
        if !e.is("collection", COL_NS) {
            return Err(
                ParseCollectionError::MissingElement(COL_NS, "collection"));
        }

        let metadata = e.get_child("metadata", COL_NS)
            .ok_or(ParseCollectionError::MissingElement(COL_NS, "metadata"))?;

        let title = metadata.get_child("title", MDML_NS)
            .ok_or(ParseCollectionError::MissingElement(MDML_NS, "title"))?
            .text();

        let content = e.get_child("content", COL_NS)
            .ok_or(ParseCollectionError::MissingElement(COL_NS, "content"))?
            .children()
            .map(Element::from_xml)
            .collect::<Result<Vec<_>, ParseCollectionError>>()?;

        Ok(Collection { title, content })
    }
}

impl Element {
    fn from_xml(e: &XmlElement) -> Result<Element, ParseCollectionError> {
        if e.is("module", COL_NS) {
            ModuleElement::from_xml(e).map(Element::Module)
        } else if e.is("subcollection", COL_NS) {
            Subcollection::from_xml(e).map(Element::Subcollection)
        } else {
            Err(ParseCollectionError::InvalidElement(
                e.ns().unwrap_or_else(String::new),
                e.name().to_string(),
            ))
        }
    }
}

impl ModuleElement {
    fn from_xml(e: &XmlElement) -> Result<ModuleElement, ParseCollectionError> {
        if !e.is("module", COL_NS) {
            return Err(
                ParseCollectionError::MissingElement(COL_NS, "module"));
        }

        let document = e.attr("document")
            .ok_or(ParseCollectionError::MissingAttribute(
                "document", COL_NS, "module"))?
            .to_string();

        let title = e.get_child("title", MDML_NS)
            .ok_or(ParseCollectionError::MissingElement(MDML_NS, "title"))?
            .text();

        Ok(ModuleElement { title, document })
    }
}

impl Subcollection {
    fn from_xml(e: &XmlElement) -> Result<Subcollection, ParseCollectionError> {
        if !e.is("subcollection", COL_NS) {
            return Err(
                ParseCollectionError::MissingElement(COL_NS, "subcollection"));
        }

        let title = e.get_child("title", MDML_NS)
            .ok_or(ParseCollectionError::MissingElement(MDML_NS, "title"))?
            .text();

        let content = e.get_child("content", COL_NS)
            .ok_or(ParseCollectionError::MissingElement(COL_NS, "content"))?
            .children()
            .map(Element::from_xml)
            .collect::<Result<Vec<_>, ParseCollectionError>>()?;

        Ok(Subcollection { title, content })
    }
}
