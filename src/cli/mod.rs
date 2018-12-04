use structopt::StructOpt;

use crate::Result;

mod server;
mod user;

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
    /// Manage users
    #[structopt(name = "user")]
    User(user::Opts),
}

pub fn main() -> Result<()> {
    let opts = Opts::from_args();
    let config = crate::config::load()?;

    match opts.command {
        Command::Start => server::start(config),
        Command::User(opts) => user::main(config, opts),
    }
}

