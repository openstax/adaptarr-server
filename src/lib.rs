extern crate actix_web;
extern crate failure;
extern crate toml;

#[macro_use] extern crate failure_derive;
#[macro_use] extern crate serde_derive;

mod api;
mod config;

pub type Result<T> = std::result::Result<T, failure::Error>;

pub fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    let config = config::load()?;

    api::start(config);

    Ok(())
}
