use diesel::prelude::*;
use failure::format_err;
use std::path::PathBuf;
use structopt::StructOpt;
use uuid::Uuid;

use crate::{
    Config,
    Result,
    db::{
        self,
        models,
        schema::{documents, drafts, modules, module_versions},
    },
    i18n::LanguageTag,
    models::{
        File,
        module::{Module, FindModuleError},
    },
    utils::bytes_to_hex,
};
use super::util::print_table;

/// Manage documents
#[derive(StructOpt)]
pub struct Opts {
    /// Document to inspect
    document: Option<Uuid>,
    #[structopt(subcommand)]
    command: Option<Command>,
}

#[derive(StructOpt)]
pub enum Command {
    /// List all documents
    #[structopt(name = "list")]
    List,
    /// List all versions of a document
    #[structopt(name = "versions")]
    Versions,
    /// Inspect a file
    #[structopt(name = "file")]
    File(FileOpts),
    /// Get contents of a file
    #[structopt(name = "cat")]
    Cat(CatOpts),
    /// Crate a new document
    #[structopt(name = "new")]
    New(NewOpts),
}

pub fn main(cfg: Config, opts: Opts) -> Result<()> {
    if opts.document.is_none() && opts.command.is_none() {
        Opts::clap().print_help()?;
        return Ok(());
    }

    if opts.command.is_none() {
        return inspect(cfg, &opts);
    }

    match opts.command.as_ref().unwrap() {
        Command::List => list(cfg),
        Command::Versions => versions(cfg, &opts),
        Command::File(file_opts) => file(cfg, &opts, file_opts),
        Command::Cat(cat_opts) => cat(cfg, &opts, cat_opts),
        Command::New(new_opts) => new(cfg, &opts, new_opts),
    }
}

fn inspect(cfg: Config, opts: &Opts) -> Result<()> {
    let db = db::connect(&cfg)?;
    let module = opts.document(&db)?;

    println!("UUID:     {}", module.id());
    println!("Title:    {}", module.title);
    println!("Language: {}", module.language);

    if let Some(user) = module.get_assignee(&db)? {
        println!("Assignee: {} ({})", user.name, user.id);
    } else {
        println!("Assignee: none");
    }

    println!("\nFiles:");

    let index = module.get_file(&db, "index.cnxml")?;
    println!("  - index.cnxml {} ({})", index.mime, index.id);

    for (name, file) in module.get_files(&db)? {
        println!("  - {} {} ({})", name, file.mime, file.id);
    }

    Ok(())
}

fn list(cfg: Config) -> Result<()> {
    let db = db::connect(&cfg)?;
    let modules = Module::all(&db)?;

    let rows = modules.iter()
        .map(|module| (module.id().to_string(), module.title.as_str()))
        .collect::<Vec<_>>();

    print_table(("UUID", "Title"), &rows);

    Ok(())
}

fn versions(cfg: Config, opts: &Opts) -> Result<()> {
    let db = db::connect(&cfg)?;
    let document = opts.document_id()?;

    let versions = module_versions::table
        .filter(module_versions::module.eq(document))
        .order_by(module_versions::version.asc())
        .inner_join(documents::table)
        .get_results::<(models::ModuleVersion, models::Document)>(&db)?;

    let mut rows = versions.into_iter()
        .enumerate()
        .map(|(version, (module, document))| (
            version.to_string(),
            module.version.to_string(),
            document.title,
        ))
        .collect::<Vec<_>>();

    let draft = drafts::table
        .filter(drafts::module.eq(document))
        .inner_join(documents::table)
        .get_result::<(models::Draft, models::Document)>(&db)
        .optional()?;

    if let Some((_, document)) = draft {
        rows.push((
            "draft".to_string(),
            "".to_string(),
            document.title,
        ));
    }

    print_table(("Ver", "Date created", "Title"), &rows);

    Ok(())
}

#[derive(StructOpt)]
pub struct FileOpts {
    name: String,
}

fn file(cfg: Config, opts: &Opts, file: &FileOpts) -> Result<()> {
    let db = db::connect(&cfg)?;
    let module = opts.document(&db)?;
    let file = module.get_file(&db, &file.name)?;
    let metadata = std::fs::metadata(&file.path)?;

    println!("Type:    {}", file.mime);
    println!("Hash:    {}", bytes_to_hex(&file.hash));
    println!("Storage: {}", file.path);
    println!("Size:    {}", metadata.len());

    Ok(())
}

#[derive(StructOpt)]
pub struct CatOpts {
    name: String,
}

fn cat(cfg: Config, opts: &Opts, cat: &CatOpts) -> Result<()> {
    let db = db::connect(&cfg)?;
    let module = opts.document(&db)?;
    let file = module.get_file(&db, &cat.name)?;
    std::io::copy(&mut file.open()?, &mut std::io::stdout())?;
    Ok(())
}

#[derive(StructOpt)]
pub struct NewOpts {
    /// Document's title
    title: String,
    /// File to use as index.cnxml
    #[structopt(short = "i", long = "index", parse(from_os_str))]
    index: PathBuf,
    /// Document's language
    #[structopt(short = "l", long = "language", alias = "lang")]
    language: LanguageTag,
}

fn new(cfg: Config, opts: &Opts, new: &NewOpts) -> Result<()> {
    let db = db::connect(&cfg)?;

    if let Some(id) = opts.document {
        match Module::by_id(&db, id) {
            Ok(_) => return Err(format_err!(
                "There is already a document with this UUID")),
            Err(FindModuleError::Database(err)) => return Err(err.into()),
            Err(FindModuleError::NotFound) => (),
        }
    }

    let document = db.transaction::<_, failure::Error, _>(|| {
        let index = std::fs::File::open(&new.index)?;
        let index = File::from_read(&db, &cfg.storage, index)?;
        let module = Module::create::<&str, _>(
            &db, &new.title, new.language.as_str(), index, std::iter::empty())?;

        if let Some(id) = opts.document {
            let (module, _) = module.into_db();

            diesel::update(&module)
                .set(modules::id.eq(id))
                .execute(&db)?;

            Ok(id)
        } else {
            Ok(module.id())
        }
    })?;

    inspect(cfg, &Opts {
        document: Some(document),
        command: None,
    })
}

impl Opts {
    fn document_id(&self) -> Result<Uuid> {
        match self.document {
            Some(uuid) => Ok(uuid),
            None => Err(format_err!("This command requires a document")),
        }
    }

    fn document(&self, db: &db::Connection) -> Result<Module> {
        Module::by_id(db, self.document_id()?).map_err(Into::into)
    }
}
