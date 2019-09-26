//! Server administration.

use actix::{Actor, System};
use actix_web::{App, HttpServer, middleware::{Compress, Logger}};
use adaptarr_models::processing::{Importer, TargetProcessor};
use adaptarr_web::{Secret, SessionManager};
use failure::Error;
use structopt::StructOpt;

use crate::Config;

#[derive(StructOpt)]
pub struct Opts {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
pub enum Command {
    /// Start the server
    #[structopt(name = "start")]
    Start,
}

pub fn main(cfg: Config, opts: Opts) -> Result<(), Error> {
    match opts.command {
        Command::Start => start(cfg),
    }
}

pub fn start(config: Config) -> Result<(), Error> {
    let system = System::new("adaptarr");

    let pool = adaptarr_models::db::configure_pool(config.model.database.as_ref())?;
    let i18n = adaptarr_i18n::load()?;
    let importer = Importer::start(pool.clone(), config.model.storage.path.clone());

    let address = config.server.address;
    let domain = config.server.domain.clone();

    let server = HttpServer::new(move ||
        App::new()
            .hostname(&config.server.domain)
            .data(i18n.clone())
            .data(pool.clone())
            .data(importer.clone())
            .data(Secret::new(&config.server.secret))
            .wrap(Logger::default())
            .wrap(SessionManager::new(&config.server.secret))
            .wrap(Compress::default())
            .configure(adaptarr_rest_api::configure)
            .configure(adaptarr_pages::configure)
    );

    // Manually start TargetProcessor to ensure stale documents are processed
    // immediately.
    TargetProcessor::start_default();

    let server = if let Some(fd) = listenfd::ListenFd::from_env().take_tcp_listener(0)? {
        server.listen(fd)?
    } else {
        server.bind(address)?
    };

    server
        .server_hostname(domain)
        .start();

    system.run()?;

    Ok(())
}
