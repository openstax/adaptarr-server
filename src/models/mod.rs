//! Data and behaviours modelled as objects.

pub mod invite;
pub mod user;

pub use self::{
    invite::Invite,
    user::User,
};
