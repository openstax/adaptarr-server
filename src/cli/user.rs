//! Commands for managing users.

use structopt::StructOpt;

use crate::{
    Config,
    Result,
    db,
    mail::Mailer,
    models::{Invite, User},
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
    /// Create an invitation
    #[structopt(name = "invite")]
    Invite(InviteOpts),
}

pub fn main(cfg: Config, opts: Opts) -> Result<()> {
    match opts.command {
        Command::Add(opts) => add_user(cfg, opts),
        Command::Invite(opts) => invite(cfg, opts),
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

#[derive(StructOpt)]
pub struct InviteOpts {
    /// User's email address
    email: String,
}

#[derive(Serialize)]
struct InviteTemplate {
    url: String,
}

pub fn invite(cfg: Config, opts: InviteOpts) -> Result<()> {
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

    Mailer::from_config(cfg.mail)?
        .send("invite", opts.email, "Invitation", &InviteTemplate { url });

    Ok(())
}
