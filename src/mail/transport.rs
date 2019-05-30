use failure::Error;
use lettre::sendmail::SendmailTransport;
use lettre_email::{EmailBuilder, Mailbox};

use super::config::{Config, Transports};

pub fn from_config(config: &Config) -> Box<dyn Transport> {
    match config.transport {
        Transports::Log => Box::new(Logger),
        Transports::Sendmail => Box::new(
            Lettre::new(config, SendmailTransport::new())),
    }
}

pub struct Message {
    pub to: Mailbox,
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// An object-safe version of [`lettre::Transport`].
pub trait Transport {
    fn send(&mut self, message: Message) -> Result<(), Error>;
}

impl Message {
    pub fn into_lettre(self) -> EmailBuilder {
        EmailBuilder::new()
            .to(self.to)
            .subject(self.subject)
            .alternative(self.html, self.text)
    }
}

/// Mail transport which does nothing except logging sent messages.
struct Logger;

impl Transport for Logger {
    fn send(&mut self, message: Message) -> Result<(), Error> {
        debug!("Message:\nTo: {}\nSubject: {}\n\n{}",
            message.to, message.subject, message.text);
        Ok(())
    }
}

/// Type implementing [`Transport`] for a wrapped [`lettre::Transport`].
struct Lettre<T> {
    sender: Mailbox,
    transport: T,
}

impl<T> Lettre<T> {
    fn new(config: &Config, inner: T) -> Self {
        Self {
            sender: config.sender.clone(),
            transport: inner,
        }
    }
}

impl<T, E> Transport for Lettre<T>
where
    T: for<'a> lettre::Transport<'a, Result = Result<(), E>>,
    Error: From<E>,
{
    fn send(&mut self, message: Message) -> Result<(), Error> {
        let mail = message.into_lettre()
            .from(self.sender.clone())
            .build()?
            .into();

        self.transport.send(mail).map_err(From::from)
    }
}
