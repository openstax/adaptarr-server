// TEMPORARY, see diesel-rs/diesel#1787.
#![allow(proc_macro_derive_resolution_fallback)]

#[macro_use] extern crate adaptarr_macros;
#[macro_use] extern crate bitflags;
#[macro_use] extern crate diesel;
#[macro_use] extern crate failure;
#[macro_use] extern crate failure_derive;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;
#[macro_use] extern crate tera;

#[cfg(not(debug_assertions))]
#[macro_use]
extern crate diesel_migrations;

pub use self::cli::main;

pub(crate) use self::config::Config;

#[macro_use] mod macros;
#[macro_use] mod multipart;

mod api;
mod cli;
mod config;
mod db;
mod events;
mod i18n;
mod import;
mod mail;
mod models;
mod permissions;
mod processing;
mod templates;
mod utils;

pub type Result<T, E=failure::Error> = std::result::Result<T, E>;
