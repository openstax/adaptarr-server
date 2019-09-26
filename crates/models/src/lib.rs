#[macro_use] extern crate diesel;

#[cfg(not(debug_assertions))]
#[macro_use]
extern crate diesel_migrations;

mod config;

pub mod audit;
pub mod db;
pub mod events;
pub mod models;
pub mod permissions;
pub mod processing;

pub use self::{
    config::Config,
    models::*,
    permissions::{PermissionBits, SystemPermissions, TeamPermissions},
};
