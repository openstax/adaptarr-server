use actix::System;
use adaptarr_models::audit;
use failure::Error;
use futures::{IntoFuture, future};
use sentry::protocol::Event;
use std::{env, mem, sync::Arc};
use structopt::StructOpt;

mod config;
mod document;
mod server;
mod user;
mod util;

use self::config::Config;

pub type Result<T, E=Error> = std::result::Result<T, E>;

pub(crate) const VERSION: &str = env!("VERSION");

#[derive(StructOpt)]
#[structopt(name = "adaptarr", no_version, version = VERSION)]
struct Opts {
    #[structopt(subcommand)]
    command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(StructOpt)]
enum Command {
    /// Manager server
    #[structopt(name = "server")]
    Server(server::Opts),
    /// Manage documents
    #[structopt(name = "document")]
    Document(document::Opts),
    /// Manage users
    #[structopt(name = "user")]
    User(user::Opts),
}

pub fn main() -> Result<(), Error> {
    let opts = Opts::from_args();
    let config = crate::config::load()?;

    setup_sentry(config);
    setup_logging(&config.logging)?;

    // Run validation after sentry and logging setup so that they can catch bugs
    // in validation.
    config.validate()?;

    // Register global configs with various services requiring them.
    config.register();

    match opts.command {
        Command::Server(opts) => server::main(config.clone(), opts),
        Command::Document(opts) => with_system(document::main, &config, opts),
        Command::User(opts) => with_system(user::main, &config, opts),
    }
}

fn setup_sentry(config: &Config) {
    if let Some(ref sentry) = config.sentry {
        env::set_var("RUST_BACKTRACE", "1");
        mem::forget(sentry::init((sentry.dsn.as_str(), sentry::ClientOptions {
            trim_backtraces: true,
            debug: cfg!(debug_assertions),
            release: Some(env!("CARGO_PKG_VERSION").into()),
            server_name: Some(config.server.domain.clone().into()),
            before_send: Some(Arc::new(Box::new(before_send_event_to_sentry))),
            .. Default::default()
        })));
        sentry::integrations::panic::register_panic_handler();
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

/// Run a function in a context of an Actix system.
fn with_system<F, O, I>(f: F, config: &Config, opts: O)
-> Result<I::Item, Error>
where
    F: FnOnce(&Config, O) -> I,
    I: IntoFuture,
    I::Error: Send + Sync,
    Error: From<I::Error>,
{
    System::new("adaptarr::cli")
        .block_on(future::lazy(|| {
            audit::set_actor(audit::Actor::System);
            f(config, opts)
        }))
        .map_err(From::from)
}

fn before_send_event_to_sentry(mut ev: Event<'static>) -> Option<Event<'static>> {
    if let Some(ref mut request) = ev.request {
        request.headers.remove("cookie");
    }
    Some(ev)
}
