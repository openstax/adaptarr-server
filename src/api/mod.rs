use actix::{Actor, Addr, System};
use actix_web::{
    App,
    middleware::Logger,
    server,
};
use sentry_actix::SentryMiddleware;

use super::{
    Result,
    config::Config,
    db,
    i18n::{self, I18n},
    import::Importer,
    processing::TargetProcessor,
};

pub use self::error::{ApiError, Error};

pub(self) use self::error::{RouteExt, RouterExt};

/// Start an API server.
pub fn start(cfg: &Config) -> Result<()> {
    let system = System::new("adaptarr");
    let state = configure(cfg.clone())?;
    let server = server::new(move || vec![
        new_app(state.clone()),
        pages::app(state.clone()),
    ]);

    // Manually start TargetProcessor to ensure stale documents are processed
    // immediately.
    TargetProcessor::start_default();

    let server = if let Some(fd) = listenfd::ListenFd::from_env().take_tcp_listener(0)? {
        server.listen(fd)
    } else {
        server.bind(cfg.server.address)?
    };

    server
        .server_hostname(cfg.server.domain.clone())
        .start();

    system.run();

    Ok(())
}
