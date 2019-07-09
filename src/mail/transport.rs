use failure::{Error, Fail};
use lettre::{
    sendmail::SendmailTransport,
    smtp::{
        self,
        ClientSecurity,
        SmtpClient,
        client::net::{DEFAULT_TLS_PROTOCOLS, ClientTlsParameters},
        error::Error as SmtpError,
    },
};
use lettre_email::{EmailBuilder, Mailbox};
use native_tls::TlsConnector;
use std::{error::Error as StdError, fmt};

use super::config::{Config, SmtpConfig, Transports};

pub fn from_config(config: &Config) -> Result<Box<dyn Transport>, Error> {
    match config.transport {
        Transports::Log => Ok(Box::new(Logger)),
        Transports::Sendmail => Ok(Box::new(
            Lettre::new(config, SendmailTransport::new(), From::from))),
        Transports::Smtp(ref cfg) => build_smtp_transport(config, cfg),
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
struct Lettre<T, H> {
    sender: Mailbox,
    transport: T,
    error_handler: H,
}

impl<T, H> Lettre<T, H> {
    fn new(config: &Config, inner: T, error_handler: H) -> Self {
        Self {
            sender: config.sender.clone(),
            transport: inner,
            error_handler,
        }
    }
}

impl<T, R, E, H> Transport for Lettre<T, H>
where
    T: for<'a> lettre::Transport<'a, Result = Result<R, E>>,
    H: Fn(E) -> Error + Copy,
{
    fn send(&mut self, message: Message) -> Result<(), Error> {
        let mail = message.into_lettre()
            .from(self.sender.clone())
            .build()?
            .into();

        self.transport.send(mail)
            .map(|_| ())
            .map_err(self.error_handler)
    }
}

fn build_smtp_transport(config: &Config, cfg: &SmtpConfig)
-> Result<Box<dyn Transport>, failure::Error> {
    let mut tls_builder = TlsConnector::builder();
    tls_builder.min_protocol_version(Some(DEFAULT_TLS_PROTOCOLS[0]));

    let tls = ClientTlsParameters::new(
        cfg.host.clone(), tls_builder.build().unwrap());

    let sec = if cfg.use_tls {
        ClientSecurity::Wrapper(tls)
    } else {
        ClientSecurity::Opportunistic(tls)
    };

    let port = match cfg.port {
        Some(port) => port,
        None if cfg.use_tls => smtp::SUBMISSIONS_PORT,
        None => smtp::SMTP_PORT,
    };

    let addr = (cfg.host.as_str(), port);

    let smtp = SmtpClient::new(addr, sec)?
        .smtp_utf8(true)
        .transport();

    Ok(Box::new(Lettre::new(config, smtp, map_smtp_error)))
}

#[allow(deprecated)] // SmtpError doesn't implement Error::source()
fn map_smtp_error(err: SmtpError) -> Error {
    if StdError::cause(&err).is_some() {
        Error::from(DescribedSmtpError(err))
    } else {
        Error::from(err)
    }
}

#[derive(Debug, Fail)]
struct DescribedSmtpError(SmtpError);

impl fmt::Display for DescribedSmtpError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        #[allow(deprecated)] // SmtpError doesn't implement Error::source()
        fmt::Display::fmt(StdError::cause(&self.0).unwrap(), fmt)
    }
}
