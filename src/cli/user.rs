//! Commands for managing users.

use diesel::Connection as _;
use failure::Fail;
use std::{collections::HashMap, str::FromStr};
use structopt::StructOpt;

use crate::{
    Config,
    Result,
    db::{self, Connection},
    i18n::{I18n, LanguageTag},
    mail::Mailer,
    models::{Invite, User, role::{Role, FindRoleError}},
    permissions::PermissionBits,
    templates,
};
use super::util::{parse_permissions, print_table};

#[derive(StructOpt)]
pub struct Opts {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
pub enum Command {
    /// List all users
    #[structopt(name = "list")]
    List,
    /// Add a new user
    #[structopt(name = "add")]
    Add(AddOpts),
    /// Create an invitation
    #[structopt(name = "invite")]
    Invite(InviteOpts),
    /// Modify a user
    #[structopt(name = "modify")]
    Modify(ModifyOpts),
}

pub fn main(cfg: Config, opts: Opts) -> Result<()> {
    match opts.command {
        Command::List => list(cfg),
        Command::Add(opts) => add_user(cfg, opts),
        Command::Invite(opts) => invite(cfg, opts),
        Command::Modify(opts) => modify(cfg, opts),
    }
}

pub fn list(cfg: Config) -> Result<()> {
    let db = db::connect(&cfg)?;
    let users = User::all(&db)?;
    let roles = Role::all(&db)?
        .into_iter()
        .map(|role| (role.id, role))
        .collect::<HashMap<_, _>>();

    let rows = users.iter()
        .map(|user| (
            user.id.to_string(),
            user.name.as_str(),
            user.email.as_str(),
            user.language.as_str(),
            match user.role {
                Some(role) => roles.get(&role)
                    .expect("database inconsistency: no role for user")
                    .name
                    .as_str(),
                None => "",
            },
        ))
        .collect::<Vec<_>>();

    print_table(("ID", "Name", "Email", "Lng", "Role"), &rows);

    Ok(())
}

#[derive(StructOpt)]
pub struct AddOpts {
    /// User's email address
    email: String,
    /// User's name
    #[structopt(long = "name", short = "n")]
    name: String,
    /// User's password
    #[structopt(long = "password", short = "p")]
    password: String,
    /// This user is an administrator
    #[structopt(long = "administrator")]
    is_super: bool,
    /// User's preferred language.
    #[structopt(long = "language", default_value = "en")]
    language: LanguageTag,
}

pub fn add_user(cfg: Config, opts: AddOpts) -> Result<()> {
    let db = db::connect(&cfg)?;
    let user = User::create(
        &db,
        &opts.email,
        &opts.name,
        &opts.password,
        opts.is_super,
        opts.language.as_str(),
        PermissionBits::normal(),
    )?;

    println!("Created user {}", user.id);

    Ok(())
}

#[derive(StructOpt)]
pub struct InviteOpts {
    /// User's email address
    email: String,
    /// Language in which to send invitation
    #[structopt(long = "lang")]
    language: LanguageTag,
}

pub fn invite(cfg: Config, opts: InviteOpts) -> Result<()> {
    let i18n = I18n::load()?;
    let locale = match i18n.find_locale(&opts.language) {
        Some(locale) => locale,
        None => return Err(InviteError::NoSuchLocale(opts.language).into()),
    };
    let db = db::connect(&cfg)?;
    let invite = Invite::create(&db, &opts.email)?;
    let code = invite.get_code(&cfg);

    println!("Invitation code: {}", code);
    println!("Registration url: {}/register?invite={}", cfg.server.domain, code);

    let code = invite.get_code(&cfg);
    // TODO: get URL from Actix.
    let url = format!(
        "https://{}/register?invite={}",
        &cfg.server.domain,
        code,
    );

    Mailer::from_config(cfg.mail)?.send(
        "invite",
        opts.email.as_str(),
        "mail-invite-subject",
        &templates::InviteMailArgs {
            url: &url,
            email: &opts.email,
        },
        locale,
    );

    Ok(())
}

#[derive(Debug, Fail)]
enum InviteError {
    #[fail(display = "No such locale: {}", _0)]
    NoSuchLocale(LanguageTag),
}

#[derive(StructOpt)]
pub struct ModifyOpts {
    user: i32,
    /// Set user's name
    #[structopt(long = "name")]
    name: Option<String>,
    /// Set user's language
    #[structopt(long = "language", alias = "lang")]
    language: Option<LanguageTag>,
    /// Set user's permissions
    #[structopt(long = "permissions", parse(try_from_str = "parse_permissions"))]
    permissions: Option<PermissionBits>,
    /// Set user's role
    #[structopt(long = "role")]
    role: Option<RoleArg>,
}

pub fn modify(cfg: Config, opts: ModifyOpts) -> Result<()> {
    let db = db::connect(&cfg)?;
    let mut user = User::by_id(&db, opts.user)?;

    db.transaction::<_, failure::Error, _>(|| {
        if let Some(name) = opts.name {
            user.set_name(&db, &name)?;
        }

        if let Some(lang) = opts.language {
            user.set_language(&db, &lang)?;
        }

        if let Some(permissions) = opts.permissions {
            user.set_permissions(&db, permissions)?;
        }

        if let Some(role) = opts.role {
            let role = role.get(&db)?;
            user.set_role(&db, role.as_ref())?;
        }

        Ok(())
    })?;

    Ok(())
}

#[derive(Debug)]
enum RoleArg {
    Null,
    ById(i32),
}

impl RoleArg {
    fn get(&self, db: &Connection) -> Result<Option<Role>, FindRoleError> {
        match self {
            RoleArg::Null => Ok(None),
            RoleArg::ById(id) => Role::by_id(db, *id).map(Some),
        }
    }
}

impl FromStr for RoleArg {
    type Err = ParseRoleArgError;

    fn from_str(v: &str) -> Result<Self, ParseRoleArgError> {
        if v == "null" || v == "nil" {
            return Ok(RoleArg::Null);
        }

        v.parse()
            .map(RoleArg::ById)
            .map_err(ParseRoleArgError)
    }
}

#[derive(Debug, Fail)]
#[fail(display = "bad role: {}. Expected number or null", _0)]
struct ParseRoleArgError(std::num::ParseIntError);
