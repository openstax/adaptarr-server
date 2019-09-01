use adaptarr_models::{PermissionBits, Role, db};
use failure::Error;
use structopt::StructOpt;

use crate::{Config, Result, util::{parse_permissions, print_table}};

#[derive(StructOpt)]
pub struct Opts {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
pub enum Command {
    /// List roles
    #[structopt(name = "list")]
    List,
    /// Add a role
    #[structopt(name = "add")]
    Add(AddOpts),
}

pub fn main(cfg: &Config, opts: Opts) -> Result<(), Error> {
    match opts.command {
        Command::List => list(cfg),
        Command::Add(opts) => add_role(cfg, opts),
    }
}

fn list(cfg: &Config) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let roles = Role::all(&db)?;

    let rows = roles.iter()
        .map(|role| (role.id.to_string(), role.name.as_str()))
        .collect::<Vec<_>>();

    print_table(("ID", "Name"), &rows);

    Ok(())
}

#[derive(StructOpt)]
pub struct AddOpts {
    /// Role's name
    name: String,
    /// Role's permissions
    #[structopt(long = "permissions", parse(try_from_str = parse_permissions))]
    permissions: Option<PermissionBits>,
}

fn add_role(cfg: &Config, opts: AddOpts) -> Result<()> {
    let db = db::connect(cfg.model.database.as_ref())?;
    let permissions = opts.permissions.unwrap_or_else(PermissionBits::empty);
    let role = Role::create(&db, &opts.name, permissions)?;

    println!("Created role {}", role.id);

    Ok(())
}
