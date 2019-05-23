//! File upload and importing ZIPs of modules and collections.

use actix::{Actor, Addr, Handler, SyncArbiter, SyncContext, Message};
use diesel::{
    Connection as _Connection,
    result::Error as DbError,
};
use failure::Fail;
use minidom::Element as XmlElement;
use std::{path::PathBuf, io::Read, str::FromStr};
use tempfile::NamedTempFile;
use zip::{ZipArchive, result::ZipError};

use crate::{
    ApiError,
    config::Storage,
    db::{Connection, Pool},
    models::{
        Book,
        bookpart::CreatePartError,
        file::{File, CreateFileError},
        module::{Module, ReplaceModuleError},
    },
    processing::{TargetProcessor, ProcessDocument},
};

/// CNX includes in its ZIP exports a number of artefacts which we have no use
/// for, and which may cause problems when importing back into CNX. This array
/// contains names of such artefacts.
const SKIP_FILES: &[&str] = &[
    "index.cnxml.html",
    "index_auto_generated.cnxml",
];

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

/// Requested a new book to be created from contents of a ZIP archive.
pub struct ImportBook {
    pub title: String,
    pub file: NamedTempFile,
}

impl Message for ImportBook {
    type Result = Result<Book, ImportError>;
}

/// Request contents of an existing book to be replaced with contents of
/// a ZIP archive.
pub struct ReplaceBook {
    pub book: Book,
    pub file: NamedTempFile,
}

impl Message for ReplaceBook {
    type Result = Result<Book, ImportError>;
}

/// Actix actor processing ZIPs in a background worker.
pub struct Importer {
    pool: Pool,
    config: Storage,
    xref_processor: Addr<TargetProcessor>,
}

impl Importer {
    pub fn new(
        pool: Pool,
        config: Storage,
        xref_processor: Addr<TargetProcessor>,
    ) -> Importer {
        Importer {
            pool,
            config,
            xref_processor,
        }
    }

    pub fn start(
        pool: Pool,
        config: Storage,
        xref_processor: Addr<TargetProcessor>,
    ) -> Addr<Importer> {
        SyncArbiter::start(1, move || Importer::new(
            pool.clone(), config.clone(), xref_processor.clone()))
    }

    /// Process a zipped module and extract index.cnxml and other media files
    /// from it.
    fn process_module_zip(&mut self, mut file: NamedTempFile)
    -> Result<ModuleZip, ImportError> {
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
                    path.parent().map_or_else(PathBuf::new, ToOwned::to_owned),
                ));
                break;
            }
        }

        let (index, base_path) = index.ok_or(ImportError::IndexMissing)?;

        let mut data = String::new();
        zip.by_index(index)?.read_to_string(&mut data)?;
        let data = XmlElement::from_str(&data)?;
        let data = Document::from_xml(&data)
            .map_err(|e| ImportError::MalformedIndexCnxml(
                "index.cnxml".to_string(), e))?;

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

            if SKIP_FILES.contains(&name.as_str()) {
                continue;
            }

            let file = File::from_read(&*db, &self.config, file)?;

            files.push((name, file));
        }

        Ok(ModuleZip {
            title: data.title,
            language: data.language,
            index: index_file,
            files,
        })
    }

    /// Create a new module from a ZIP of its contents.
    fn create_module(&mut self, title: String, file: NamedTempFile)
    -> Result<Module, ImportError> {
        let ModuleZip {
            language, index, files, ..
        } = self.process_module_zip(file)?;

        let db = self.pool.get()?;
        let module = Module::create(&*db, &title, &language, index, files)?;

        if let Err(err) = self.xref_processor.try_send(ProcessDocument {
            document: (**module).clone()
        }) {
            error!("Could not send document {} for processing: {}",
                module.id, err);
        }

        Ok(module)
    }

    /// Import a zipped module onto an existing one.
    fn replace_module(&mut self, mut module: Module, file: NamedTempFile)
    -> Result<Module, ImportError> {
        let ModuleZip { index, files, .. } = self.process_module_zip(file)?;

        let db = self.pool.get()?;
        module.replace(&*db, index, files)?;

        if let Err(err) = self.xref_processor.try_send(ProcessDocument {
            document: (**module).clone()
        }) {
            error!("Could not send document {} for processing: {}",
                module.id, err);
        }

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
                    path.parent().map_or_else(PathBuf::new, ToOwned::to_owned),
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

    /// Import a single module from a collection ZIP.
    fn load_collection_module(
        &mut self,
        dbconn: &Connection,
        zip: &mut ZipArchive<&mut std::fs::File>,
        base_path: PathBuf,
    ) -> Result<ModuleZip, ImportError> {
        let index_path = base_path.join("index.cnxml");
        let index_path = index_path.to_str().unwrap();

        let mut data = String::new();
        zip.by_name(index_path)?.read_to_string(&mut data)?;
        let data = XmlElement::from_str(&data)?;
        let data = Document::from_xml(&data)
            .map_err(|e| ImportError::MalformedIndexCnxml(
                index_path.to_string(), e))?;

        let index_file = {
            let index = zip.by_name(index_path)?;
            File::from_read(dbconn, &self.config, index)?
        };

        let mut files = Vec::new();

        for inx in 0..zip.len() {
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

            // Don't import index.cnxml twice.
            if name == "index.cnxml" {
                continue;
            }

            if SKIP_FILES.contains(&name.as_str()) {
                continue;
            }

            let file = File::from_read(dbconn, &self.config, file)?;
            files.push((name, file));
        }

        Ok(ModuleZip {
            title: data.title,
            language: data.language,
            index: index_file,
            files,
        })
    }

    /// Load contents of a book from a collection ZIP.
    fn load_collection_zip(
        &mut self,
        dbconn: &Connection,
        book: &mut Book,
        mut zip: ZipArchive<&mut std::fs::File>,
        coldata: Collection,
        base: PathBuf,
    ) -> Result<(), ImportError> {
        let root = book.root_part(dbconn)?;

        let mut queue = vec![(root, &coldata.content)];

        while let Some((group, content)) = queue.pop() {
            for (inx, element) in content.iter().enumerate() {
                match element {
                    Element::Module(ModuleElement { title, document }) => {
                        let path = base.join(document);
                        let ModuleZip {
                            language, index, files, ..
                        } = self.load_collection_module(
                            dbconn, &mut zip, path)?;
                        let module = Module::create(dbconn, &title, &language, index, files)?;
                        group.insert_module(dbconn, inx as i32, &title, &module)
                            .map_err(|e| match e {
                                CreatePartError::Database(e) => e,
                                CreatePartError::IsAModule => unreachable!(),
                            })?;

                        if let Err(err) = self.xref_processor.try_send(
                            ProcessDocument { document: (**module).clone() })
                        {
                            error!(
                                "Could not send document {} for processing: {}",
                                module.id,
                                err,
                            );
                        }
                    }
                    Element::Subcollection(Subcollection { title, content }) => {
                        let new = group.create_group(dbconn, inx as i32, &title)
                            .map_err(|e| match e {
                                CreatePartError::Database(e) => e,
                                CreatePartError::IsAModule => unreachable!(),
                            })?;
                        queue.push((new, content));
                    }
                }
            }
        }

        Ok(())
    }

    /// Create a new book for a ZIP of its contents.
    fn create_book(&mut self, title: String, mut file: NamedTempFile)
    -> Result<Book, ImportError> {
        let (zip, coldata, base) = self.preprocess_collection_zip(&mut file)?;

        let db = self.pool.get()?;
        let dbconn = &*db;

        dbconn.transaction(|| {
            let mut book = Book::create(dbconn, &title)?;
            self.load_collection_zip(dbconn, &mut book, zip, coldata, base)?;
            Ok(book)
        })
    }

    /// Replace contents of a book from a collection ZIP.
    fn replace_book(&mut self, mut book: Book, mut file: NamedTempFile)
    -> Result<Book, ImportError> {
        let (zip, coldata, base) = self.preprocess_collection_zip(&mut file)?;

        let db = self.pool.get()?;
        let dbconn = &*db;

        dbconn.transaction(|| {
            book.root_part(dbconn)?.clear(dbconn)?;
            self.load_collection_zip(dbconn, &mut book, zip, coldata, base)?;
            Ok(book)
        })
    }
}

#[derive(Debug)]
struct ModuleZip {
    title: String,
    language: String,
    index: File,
    files: Vec<(String, File)>,
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

impl Handler<ImportBook> for Importer {
    type Result = Result<Book, ImportError>;

    fn handle(&mut self, msg: ImportBook, _: &mut Self::Context) -> Self::Result {
        let ImportBook { title, file } = msg;

        self.create_book(title, file)
    }
}

impl Handler<ReplaceBook> for Importer {
    type Result = Result<Book, ImportError>;

    fn handle(&mut self, msg: ReplaceBook, _: &mut Self::Context) -> Self::Result {
        let ReplaceBook { book, file } = msg;

        self.replace_book(book, file)
    }
}

#[derive(ApiError, Debug, Fail)]
pub enum ImportError {
    /// There was a problem with the ZIP archive.
    #[fail(display = "{}", _0)]
    #[api(code = "import:zip:invalid", status = "BAD_REQUEST")]
    Archive(#[cause] ZipError),
    /// There was no file named index.cnxml in the ZIP archive.
    #[fail(display = "Archive is missing index.cnxml")]
    #[api(code = "import:zip:index-missing", status = "BAD_REQUEST")]
    IndexMissing,
    /// There was no file named collection.xml in the ZIP archive.
    #[fail(display = "Archive is missing collection.xml")]
    #[api(code = "import:zip:collection-xml-missing", status = "BAD_REQUEST")]
    ColxmlMissing,
    /// There was a problem obtaining database connection.
    #[fail(display = "Cannot obtain database connection: {}", _0)]
    #[api(internal)]
    DbPool(#[cause] r2d2::Error),
    /// A file could not be created.
    #[fail(display = "Cannot create file: {}", _0)]
    FileCreation(#[cause] CreateFileError),
    /// Database error.
    #[fail(display = "Database error: {}", _0)]
    #[api(internal)]
    Database(#[cause] DbError),
    /// Replacing module's contents failed.
    #[fail(display = "{}", _0)]
    ReplaceModule(#[cause] ReplaceModuleError),
    /// One of the XML files was invalid.
    #[fail(display = "Invalid XML: {}", _0)]
    #[api(code = "import:invalid-xml", status = "BAD_REQUEST")]
    InvalidXml(#[cause] minidom::Error),
    /// collection.xml did not conform to schema.
    #[fail(display = "Invalid collection.xml: {}", _0)]
    #[api(code = "import:invalid-xml", status = "BAD_REQUEST")]
    MalformedColXml(#[cause] ParseCollectionError),
    /// index.cnxml did not conform to schema.
    #[fail(display = "invalid {}: {}", _0, _1)]
    #[api(code = "import:invalid-xml", status = "BAD_REQUEST")]
    MalformedIndexCnxml(String, #[cause] ParseDocumentError),
    /// An operating system error.
    #[fail(display = "System error: {}", _0)]
    #[api(internal)]
    System(#[cause] std::io::Error),
}

impl_from! { for ImportError ;
    r2d2::Error => |e| ImportError::DbPool(e),
    ZipError => |e| ImportError::Archive(e),
    CreateFileError => |e| ImportError::FileCreation(e),
    DbError => |e| ImportError::Database(e),
    ReplaceModuleError => |e| ImportError::ReplaceModule(e),
    minidom::Error => |e| ImportError::InvalidXml(e),
    ParseCollectionError => |e| ImportError::MalformedColXml(e),
    std::io::Error => |e| ImportError::System(e),
}

#[derive(Debug)]
struct Document {
    title: String,
    language: String,
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
pub enum ParseDocumentError {
    #[fail(display = "Missing required element {} from namespace {}", _1, _0)]
    MissingElement(&'static str, &'static str),
    #[fail(
        display = "Missing required attribute {} of element {} from namespace {}",
        _0, _2, _1,
    )]
    MissingAttribute(&'static str, &'static str, &'static str),
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

const CNXML_NS: &str = "http://cnx.rice.edu/cnxml";
const COL_NS: &str = "http://cnx.rice.edu/collxml";
const MDML_NS: &str = "http://cnx.rice.edu/mdml";

impl Document {
    fn from_xml(e: &XmlElement) -> Result<Document, ParseDocumentError> {
        if !e.is("document", CNXML_NS) {
            return Err(
                ParseDocumentError::MissingElement(CNXML_NS, "document"));
        }

        let language = match e.attr("xml:lang") {
            Some(attr) => attr.to_string(),
            None => {
                let metadata = e.get_child("metadata", CNXML_NS)
                    .ok_or(ParseDocumentError::MissingElement(CNXML_NS, "metadata"))?;

                metadata.get_child("language", MDML_NS)
                    .ok_or(ParseDocumentError::MissingElement(MDML_NS, "language"))?
                    .text()
            }
        };

        let title = e.get_child("title", CNXML_NS)
            .ok_or(ParseDocumentError::MissingElement(CNXML_NS, "title"))?
            .text();

        Ok(Document { language, title })
    }
}

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
