use lettre::{EmailTransport, SendmailTransport};
use lettre_email::{Email, EmailBuilder, IntoMailbox, Mailbox};
use serde::{Deserializer, Serialize};
use std::{cell::RefCell, collections::HashMap};

use crate::{
    Result,
    i18n::Locale,
    templates::{LocalizedTera, MAILS},
};

pub struct Mailer {
    config: Config,
    transport: RefCell<Transport>,
}

/// Mail system configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// Email address to send messages as.
    #[serde(deserialize_with = "de_mailbox")]
    pub sender: Mailbox,
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

    pub fn send<M, C, S>(
        &self,
        template: &str,
        to: M,
        subject: S,
        context: &C,
        locale: &'static Locale<'static>,
    )
    where
        M: IntoMailbox,
        C: Serialize,
        S: IntoSubject,
    {
        let subject = subject.into_subject(locale);

        let html = MAILS.render_i18n(
                &format!("{}.html", template), context, locale)
            .expect("template to render");
        let text = MAILS.render_i18n(
                &format!("{}.txt", template), context, locale)
            .expect("template to render");

        self.transport.borrow_mut()
            .send(&self.config, to.into_mailbox(), &subject, html, text);
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
        .from(config.sender.clone())
        .subject(subject)
        .alternative(html, text)
        .build()
        .expect("email to build correctly")
}

fn de_mailbox<'de, D>(d: D) -> std::result::Result<Mailbox, D::Error>
where
    D: Deserializer<'de>,
{
    d.deserialize_str(MailboxVisitor)
}

struct MailboxVisitor;

impl<'de> serde::de::Visitor<'de> for MailboxVisitor {
    type Value = Mailbox;

    fn expecting(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "an email address")
    }

    fn visit_str<E>(self, v: &str) -> std::result::Result<Mailbox, E>
    where
        E: serde::de::Error,
    {
        use serde::de::Unexpected;

        v.parse()
            .map_err(|_| E::invalid_value(Unexpected::Str(v), &"an email address"))
    }
}

pub trait IntoSubject {
    fn into_subject(self, locale: &Locale) -> String;
}

impl<'a> IntoSubject for &'a str {
    fn into_subject(self, locale: &Locale) -> String {
        IntoSubject::into_subject((self, &HashMap::new()), locale)
    }
}

impl<'a> IntoSubject for (&'a str, &'a HashMap<&str, fluent::FluentValue>) {
    fn into_subject(self, locale: &Locale) -> String {
        let (key, args) = self;
        match locale.format(key, args) {
            Some(subject) => subject,
            None => {
                error!("Message {} is missing from locale {}",
                    key, locale.code);
                key.to_string()
            }
        }
    }
}
