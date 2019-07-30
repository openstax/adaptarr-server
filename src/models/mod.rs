//! Data and behaviours modelled as objects.

pub mod book;
pub mod bookpart;
pub mod conversation;
pub mod document;
pub mod draft;
pub mod editing;
pub mod event;
pub mod file;
pub mod invite;
pub mod module;
pub mod password;
pub mod resource;
pub mod role;
pub mod user;
pub mod xref_target;

pub use self::{
    book::Book,
    bookpart::BookPart,
    document::Document,
    draft::Draft,
    event::Event,
    file::File,
    invite::Invite,
    module::Module,
    password::PasswordResetToken,
    resource::Resource,
    role::Role,
    user::User,
    xref_target::XrefTarget,
};
