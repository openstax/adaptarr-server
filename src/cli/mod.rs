use structopt::StructOpt;

use crate::Result;

mod server;

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
}

pub fn main() -> Result<()> {
    let opts = Opts::from_args();
    let config = crate::config::load()?;

    match opts.command {
        Command::Start => server::start(config),
    }
}

