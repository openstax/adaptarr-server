// TEMPORARY, see diesel-rs/diesel#1787.
#![allow(proc_macro_derive_resolution_fallback)]

extern crate actix_web;
extern crate chrono;
extern crate failure;
extern crate r2d2;
extern crate r2d2_diesel;
extern crate rand;
extern crate ring;
extern crate rmp_serde as rmps;
extern crate serde;
extern crate toml;

#[macro_use] extern crate diesel;
#[macro_use] extern crate failure_derive;
#[macro_use] extern crate serde_derive;

#[cfg(not(debug_assertions))]
#[macro_use]
extern crate diesel_migrations;

pub(crate) use self::config::Config;

#[macro_use] mod macros;

mod api;
mod config;
mod db;
mod utils;

pub type Result<T> = std::result::Result<T, failure::Error>;

pub fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    let config = config::load()?;

    api::start(config)?;

    Ok(())
}
