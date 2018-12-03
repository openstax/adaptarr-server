extern crate actix_web;
extern crate failure;

#[macro_use] extern crate failure_derive;

mod api;

pub type Result<T> = std::result::Result<T, failure::Error>;

pub fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    api::start();

    Ok(())
}
