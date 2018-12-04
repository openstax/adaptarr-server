//! Commands for managing users.

use structopt::StructOpt;

use crate::{
    Config,
    Result,
    db,
    models::User,
};

#[derive(StructOpt)]
pub struct Opts {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
pub enum Command {
    /// Add a new user
    #[structopt(name = "add")]
    Add(AddOpts),
}

pub fn main(cfg: Config, opts: Opts) -> Result<()> {
    match opts.command {
        Command::Add(opts) => add_user(cfg, opts),
    }
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
}

pub fn add_user(cfg: Config, opts: AddOpts) -> Result<()> {
    let db = db::connect(&cfg)?;
    let user = User::create(
        &db, &opts.email, &opts.name, &opts.password, opts.is_super)?;

    println!("Created user {}", user.id);

    Ok(())
}
