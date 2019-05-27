use actix::{Addr, System};
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
    events::{self as event_manager, EventManager},
    i18n::I18n,
    import::Importer,
    mail::Mailer,
};

pub use self::error::{ApiError, Error};

pub(self) use self::error::{RouteExt, RouterExt};

pub mod books;
pub mod conversations;
pub mod drafts;
pub mod error;
pub mod events;
pub mod modules;
pub mod pages;
pub mod process;
pub mod roles;
pub mod session;
pub mod users;
pub mod util;

/// Start an API server.
pub fn start(cfg: &Config) -> Result<()> {
    let system = System::new("adaptarr");
    let state = configure(cfg.clone())?;
    let server = server::new(move || vec![
        new_app(state.clone()),
        pages::app(state.clone()),
    ]);

    let server = if let Some(fd) = listenfd::ListenFd::from_env().take_tcp_listener(0).unwrap() {
        server.listen(fd)
    } else {
        server.bind(cfg.server.address).unwrap()
    };

    server
        .server_hostname(cfg.server.domain.clone())
        .start();

    system.run();

    Ok(())
}

#[derive(Clone)]
pub struct State {
    /// Current configuration.
    pub config: Config,
    /// Database connection pool.
    pub db: db::Pool,
    /// Mailer service.
    pub mailer: Mailer,
    /// Event manager.
    pub events: Addr<EventManager>,
    /// Localization subsystem.
    pub i18n: I18n<'static>,
    /// ZIP importer.
    pub importer: Addr<Importer>,
}

pub fn configure(cfg: Config) -> Result<State> {
    let i18n = I18n::load()?;
    let db = db::pool()?;

    Ok(State {
        config: cfg.clone(),
        db: db.clone(),
        mailer: Mailer::from_config(cfg.mail.clone())?,
        events: event_manager::start(db.clone()),
        i18n,
        importer: Importer::start(db.clone(), cfg.storage.clone()),
    })
}

pub fn new_app(state: State) -> App<State> {
    let sessions = session::SessionManager::new(
        state.config.server.secret.clone(),
        state.db.clone(),
    );

    App::with_state(state)
        .middleware(SentryMiddleware::new())
        .middleware(Logger::default())
        .middleware(sessions)
        .prefix("/api/v1")
        .configure(books::routes)
        .configure(conversations::routes)
        .configure(drafts::routes)
        .configure(events::routes)
        .configure(modules::routes)
        .configure(process::routes)
        .configure(roles::routes)
        .configure(users::routes)
}
