//! Adaptarr's REST API.

use actix_web::web::{self, ServiceConfig};

mod books;
mod config;
mod conversations;
mod drafts;
mod events;
mod modules;
mod process;
mod resources;
mod support;
mod teams;
mod users;

pub use self::config::Config;

pub type Result<T, E=adaptarr_error::Error> = std::result::Result<T, E>;

/// Configure [`App`] for an API server.
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .configure(books::configure)
            .configure(conversations::configure)
            .configure(drafts::configure)
            .configure(events::configure)
            .configure(modules::configure)
            .configure(process::configure)
            .configure(resources::configure)
            .configure(support::configure)
            .configure(teams::configure)
            .configure(users::configure)
    );
}
