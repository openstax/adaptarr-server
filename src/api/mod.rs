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
    let server = server::new(|| vec![
        api_app(),
        pages::app(),
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

fn api_app() -> App {
    App::new()
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
