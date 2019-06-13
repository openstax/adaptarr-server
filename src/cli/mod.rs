use actix::System;
use sentry::protocol::Event;
use std::{env, mem, sync::Arc};
use structopt::StructOpt;

use crate::{Result, config::Config};

mod document;
mod role;
mod server;
mod user;
mod util;

#[derive(StructOpt)]
#[structopt(raw(version = r#"env!("VERSION")"#))]
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
    /// Manage roles
    #[structopt(name = "role")]
    Role(role::Opts),
    /// Manage users
    #[structopt(name = "user")]
    User(user::Opts),
}

pub fn main() -> Result<()> {
    let opts = Opts::from_args();
    let config = crate::config::load()?;

    setup_sentry(&config)?;
    setup_logging(&config.logging)?;

    match opts.command {
        Command::Start => server::start(config),
        Command::Document(opts) => with_system(document::main, config, opts),
        Command::Role(opts) => with_system(role::main, config, opts),
        Command::User(opts) => with_system(user::main, config, opts),
    }
}

fn setup_sentry(config: &Config) -> Result<()> {
    if let Some(ref sentry) = config.sentry {
        env::set_var("RUST_BACKTRACE", "1");
        mem::forget(sentry::init((sentry.dsn.as_str(), sentry::ClientOptions {
            trim_backtraces: true,
            release: Some(env!("CARGO_PKG_VERSION").into()),
            server_name: Some(config.server.domain.clone().into()),
            before_send: Some(Arc::new(Box::new(before_send_event_to_sentry))),
            .. Default::default()
        })));
        sentry::integrations::panic::register_panic_handler();
    }

    Ok(())
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
fn with_system<F, O, R>(f: F, config: &Config, opts: O) -> R
where
    F: FnOnce(&Config, O) -> R,
{
    let system = System::new("adaptarr::cli");

    let r = f(config, opts);

    // Send stop signal to the system. Without this system.run() will hang
    // forever.
    System::current().stop();
    // Process all pending messages.
    system.run();

    r
}

fn before_send_event_to_sentry(mut ev: Event<'static>) -> Option<Event<'static>> {
    if let Some(ref mut request) = ev.request {
        request.headers.remove("cookie");
    }
    Some(ev)
}
