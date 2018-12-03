use actix_web::{
    App,
    middleware::Logger,
    server,
};

use super::{
    Result,
    config::Config,
    db,
};

mod bookparts;
mod books;
mod conversations;
mod dashboard;
mod drafts;
mod events;
mod modules;
mod pages;
mod users;

/// Start an API server.
pub fn start(cfg: Config) -> Result<()> {
    let state = State {
        config: cfg.clone(),
        db: db::pool(&cfg)?,
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
        .run();
    Ok(())
}

#[derive(Clone)]
pub struct State {
    /// Current configuration.
    pub config: Config,
    /// Database connection pool.
    pub db: db::Pool,
}

fn api_app(state: State) -> App<State> {
    App::with_state(state)
        .middleware(Logger::default())
        .prefix("/api/v1")
        .configure(bookparts::routes)
        .configure(books::routes)
        .configure(conversations::routes)
        .configure(dashboard::routes)
        .configure(drafts::routes)
        .configure(events::routes)
        .configure(modules::routes)
        .configure(users::routes)
}
