use structopt::StructOpt;

use crate::Result;

mod document;
mod server;
mod user;
mod util;

#[derive(StructOpt)]
struct Opts {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
enum Command {
    /// Start the server
    #[structopt(name = "start")]
    Start,
    #[structopt(name = "document")]
    Document(document::Opts),
    /// Manage users
    #[structopt(name = "user")]
    User(user::Opts),
}

pub fn main() -> Result<()> {
    let opts = Opts::from_args();
    let config = crate::config::load()?;

    setup_logging(&config.logging)?;

    match opts.command {
        Command::Start => server::start(config),
        Command::Document(opts) => document::main(config, opts),
        Command::User(opts) => user::main(config, opts),
    }
}

fn setup_logging(config: &crate::config::Logging) -> Result<()> {
    let mut builder = env_logger::Builder::from_default_env();
    builder.filter_level(config.level);

    if let Some(level) = config.network {
        builder.filter_module("actix_web", level);
    }

    for (module, level) in &config.filters {
        builder.filter_module(&module, *level);
    }

    builder.try_init()?;
    Ok(())
}
