use actix::{Addr, System};
use actix_web::{
    App,
    middleware::Logger,
    server,
};

use super::{
    Result,
    config::Config,
    db,
    events::{self as event_manager, EventManager},
    import::Importer,
    mail::Mailer,
    processing::TargetProcessor,
};

pub use self::error::{ApiError, Error};

pub(self) use self::error::{RouteExt, RouterExt};

mod books;
mod conversations;
mod drafts;
mod error;
mod events;
mod modules;
mod pages;
mod session;
mod users;

/// Start an API server.
pub fn start(cfg: Config) -> Result<()> {
    let system = System::new("adaptarr");

    let db = db::pool(&cfg)?;
    let xref_processor = TargetProcessor::start(db.clone());

    let state = State {
        config: cfg.clone(),
        db: db.clone(),
        mailer: Mailer::from_config(cfg.mail.clone())?,
        events: event_manager::start(db.clone()),
        importer: Importer::start(
            db.clone(), cfg.storage.clone(), xref_processor.clone()),
        xref_processor,
    };

    let server = server::new(move || vec![
        api_app(state.clone()),
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
    /// ZIP importer.
    pub importer: Addr<Importer>,
    /// Cross-reference processing service.
    pub xref_processor: Addr<TargetProcessor>,
}

fn api_app(state: State) -> App<State> {
    let sessions = session::SessionManager::new(
        state.config.server.secret.clone(),
        state.db.clone(),
    );

    App::with_state(state)
        .middleware(Logger::default())
        .middleware(sessions)
        .prefix("/api/v1")
        .configure(books::routes)
        .configure(conversations::routes)
        .configure(drafts::routes)
        .configure(events::routes)
        .configure(modules::routes)
        .configure(users::routes)
}
