//! Data and behaviours modelled as objects.

pub mod book;
pub mod file;
pub mod invite;
pub mod module;
pub mod password;
pub mod user;

pub use self::{
    book::Book,
    file::File,
    invite::Invite,
    module::Module,
    password::PasswordResetToken,
    user::User,
};
