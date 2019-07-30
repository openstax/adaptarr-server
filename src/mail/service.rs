use actix::{Actor, Context, Handler, Supervised, SystemService, dev::Request};
use futures::future::{self, Either, Future, IntoFuture};
use serde::Serialize;
use std::collections::HashMap;
use lettre_email::Mailbox;

use crate::{Result, i18n::Locale, templates::{LocalizedTera, MAILS}};
use super::transport::{self, Message, Transport};

pub struct Mailer {
    transport: Box<dyn Transport>,
}

impl Mailer {
    /// Try to send an email message.
    ///
    /// Errors will be logged, but otherwise ignored.
    pub fn do_send<M, S, C>(
        to: M,
        template: &str,
        subject: S,
        context: C,
        locale: &'static Locale<'static>,
    )
    where
        M: Into<Mailbox>,
        S: IntoSubject,
        C: Serialize,
    {
        let mailer = Mailer::from_registry();
        let message = match format_message(to, template, subject, context, locale) {
            Ok(message) => message,
            Err(err) => {
                error!("Could not format message: {}", err);
                return;
            }
        };

        if let Err(err) = mailer.try_send(message) {
            error!("Could not send mail: {}", err);
        }
    }

    /// Send an email message.
    // NOTE: This method cannot use a generic return type (impl IntoFuture<...>)
    // as this messes up lifetime inference requiring all parameters to
    // be 'static.
    pub fn send<M, S, C>(
        to: M,
        template: &str,
        subject: S,
        context: C,
        locale: &'static Locale<'static>,
    ) -> SendFuture
    where
        M: Into<Mailbox>,
        S: IntoSubject,
        C: Serialize,
    {
        let mailer = Mailer::from_registry();
        let message = match format_message(to, template, subject, context, locale) {
            Ok(message) => message,
            Err(err) => return SendFuture(Either::A(future::err(err))),
        };

        SendFuture(Either::B(mailer.send(message).map_err(From::from)))
    }
}

pub struct SendFuture(
    Either<
        future::FutureResult<(), failure::Error>,
        future::MapErr<
            Request<Mailer, Message>,
            fn(actix::dev::MailboxError) -> failure::Error,
        >,
    >
);

impl IntoFuture for SendFuture {
    type Future = Either<
        future::FutureResult<(), failure::Error>,
        future::MapErr<
            Request<Mailer, Message>,
            fn(actix::dev::MailboxError) -> failure::Error,
        >,
    >;
    type Item = ();
    type Error = failure::Error;

    fn into_future(self) -> Self::Future {
        self.0
    }
}

fn format_message<M, S, C>(
    to: M,
    template: &str,
    subject: S,
    context: C,
    locale: &'static Locale<'static>,
) -> Result<Message>
where
    M: Into<Mailbox>,
    S: IntoSubject,
    C: Serialize,
{
    let subject = subject.into_subject(locale);

    let template_html = format!("{}.html", template);
    let template_text = format!("{}.txt", template);

    Ok(Message {
        to: to.into(),
        subject,
        html: MAILS.render_i18n(&template_html, &context, locale)?,
        text: MAILS.render_i18n(&template_text, &context, locale)?,
    })
}

impl Default for Mailer {
    fn default() -> Self {
        let config = crate::config::load()
            .expect("Configuration should be ready when mailer is started");

        let transport = transport::from_config(&config.mail)
            .expect("Transport configuration should already be validated when \
                mailer is started");

        Self { transport }
    }
}

impl Actor for Mailer {
    type Context = Context<Self>;
}

impl Supervised for Mailer {
}

impl SystemService for Mailer {
}

impl actix::Message for Message {
    type Result = ();
}

impl Handler<Message> for Mailer {
    type Result = ();

    fn handle(&mut self, msg: Message, _: &mut Self::Context) {
        match self.transport.send(msg) {
            Ok(()) => (),
            Err(err) => {
                error!("Could not send email: {}", err);
            }
        }
    }
}

/// A type that can be converted into a message subject.
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
