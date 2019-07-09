use failure::Error;
use lettre::{
    sendmail::SendmailTransport,
    smtp::{
        self,
        ClientSecurity,
        SmtpClient,
        client::net::{DEFAULT_TLS_PROTOCOLS, ClientTlsParameters},
    },
};
use lettre_email::{EmailBuilder, Mailbox};
use native_tls::TlsConnector;

use super::config::{Config, SmtpConfig, Transports};

pub fn from_config(config: &Config) -> Result<Box<dyn Transport>, Error> {
    match config.transport {
        Transports::Log => Ok(Box::new(Logger)),
        Transports::Sendmail => Ok(Box::new(
            Lettre::new(config, SendmailTransport::new()))),
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

impl<T, R, E> Transport for Lettre<T>
where
    T: for<'a> lettre::Transport<'a, Result = Result<R, E>>,
    Error: From<E>,
{
    fn send(&mut self, message: Message) -> Result<(), Error> {
        let mail = message.into_lettre()
            .from(self.sender.clone())
            .build()?
            .into();

        self.transport.send(mail)
            .map(|_| ())
            .map_err(From::from)
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

    Ok(Box::new(Lettre::new(config, smtp)))
}
