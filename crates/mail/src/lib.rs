mod config;
mod service;
mod transport;

pub use self::{
    config::Config,
    service::{Mailer, SendFuture, IntoSubject},
};

pub use lettre_email::Mailbox;
