use lettre::{EmailTransport, SendmailTransport};
use lettre_email::{Email, EmailBuilder, IntoMailbox, Mailbox};
use serde::Serialize;
use std::cell::RefCell;

use crate::{
    Result,
    templates::MAILS,
};

pub struct Mailer {
    config: Config,
    transport: RefCell<Transport>,
}

/// Mail system configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// Email address to send messages as.
    pub sender: String,
    /// Transport method to use, and its configuration.
    #[serde(flatten)]
    pub transport: Transports,
}

/// Mail transport configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum Transports {
    /// Log messages to standard error.
    Log,
    /// Use the `sendmail(1)` command.
    Sendmail,
}

impl Mailer {
    pub fn from_config(config: Config) -> Result<Mailer> {
        let transport = match config.transport {
            Transports::Log => Transport::Log,
            Transports::Sendmail => Transport::Sendmail(SendmailTransport::new()),
        };

        Ok(Mailer {
            config,
            transport: RefCell::new(transport),
        })
    }

    pub fn send<M, C>(
        &self,
        template: &str,
        to: M,
        subject: &str,
        context: &C
    )
    where
        M: IntoMailbox,
        C: Serialize,
    {
        let html = MAILS.render(&format!("{}.html", template), context)
            .expect("template to render");
        let text = MAILS.render(&format!("{}.txt", template), context)
            .expect("template to render");

        self.transport.borrow_mut()
            .send(&self.config, to.into_mailbox(), subject, html, text);
    }
}

impl Clone for Mailer {
    fn clone(&self) -> Mailer {
        Mailer::from_config(self.config.clone())
            .expect("cannot recreate mailer")
    }
}

enum Transport {
    Log,
    Sendmail(SendmailTransport),
}

impl Transport {
    fn send(
        &mut self,
        config: &Config,
        to: Mailbox,
        subject: &str,
        html: String,
        text: String,
    ) {
        match *self {
            Transport::Log => log_mail(to, subject, &text),
            Transport::Sendmail(ref mut t) =>
                t.send(&construct(config, to, subject, &html, &text))
                    .expect("mail to be sent"),
        }
    }
}

fn log_mail(to: Mailbox, subject: &str, text: &str) {
    eprintln!("To: {}\nSubject: {}\n{}", to, subject, text);
}

fn construct(
    config: &Config,
    to: Mailbox,
    subject: &str,
    html: &str,
    text: &str,
) -> Email {
    EmailBuilder::new()
        .to(to)
        .from(config.sender.as_str())
        .subject(subject)
        .alternative(html, text)
        .build()
        .expect("email to build correctly")
}
