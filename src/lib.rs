// TEMPORARY, see diesel-rs/diesel#1787.
#![allow(proc_macro_derive_resolution_fallback)]

extern crate actix;
extern crate actix_web;
extern crate argon2;
extern crate base64;
extern crate blake2_rfc as blake2;
extern crate bytes;
extern crate chrono;
extern crate failure;
extern crate futures;
extern crate lettre;
extern crate lettre_email;
extern crate magic;
extern crate minidom;
extern crate r2d2;
extern crate r2d2_diesel;
extern crate rand;
extern crate ring;
extern crate rmp_serde as rmps;
extern crate serde;
extern crate serde_json;
extern crate structopt;
extern crate tempfile;
extern crate toml;
extern crate uuid;
extern crate zip;

#[macro_use] extern crate api_derive;
#[macro_use] extern crate bitflags;
#[macro_use] extern crate diesel;
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
mod import;
mod mail;
mod models;
mod permissions;
mod processing;
mod templates;
mod utils;

pub type Result<T> = std::result::Result<T, failure::Error>;
