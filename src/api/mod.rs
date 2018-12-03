use actix_web::{
    App,
    middleware::Logger,
    server,
};

use super::config::Config;

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
pub fn start(cfg: Config) {
    let state = State {
        config: cfg.clone(),
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
        .run()
}

#[derive(Clone)]
pub struct State {
    pub config: Config,
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
