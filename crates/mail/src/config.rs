use lettre_email::Mailbox;
use serde::{Deserialize, Deserializer, de};
use std::fmt;
use adaptarr_util::SingleInit;

static CONFIG: SingleInit<&'static Config> = SingleInit::uninit();

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

impl Config {
    /// Get global configuration.
    ///
    /// ## Panics
    ///
    /// This function will panic if called before [`Config::register`].
    pub fn global() -> &'static Config {
        CONFIG.get().expect("mailing configuration must be initialized before \
            calling Config::global")
    }

    /// Validate configuration correctness.
    pub fn validate(&self) -> Result<(), failure::Error> {
        super::transport::from_config(self)?;
        Ok(())
    }

    /// Register this configuration as the global static configuration
    /// ([`Config::global`]).
    pub fn register(&'static self) {
        CONFIG.get_or_init(|| self);
    }
}

/// Mail transport configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum Transports {
    /// Log messages to standard error.
    Log,
    /// Use the `sendmail(1)` command.
    Sendmail,
    /// Use SMTP
    Smtp(SmtpConfig),
}

/// SMTP configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SmtpConfig {
    /// The host name to connect to.
    pub host: String,
    #[serde(default)]
    /// The port to connect to.
    pub port: Option<u16>,
    /// Should we force TLS?
    pub use_tls: UseTls,
}

fn de_mailbox<'de, D>(d: D) -> std::result::Result<Mailbox, D::Error>
where
    D: Deserializer<'de>,
{
    d.deserialize_str(MailboxVisitor)
}

struct MailboxVisitor;

impl<'de> de::Visitor<'de> for MailboxVisitor {
    type Value = Mailbox;

    fn expecting(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "an email address")
    }

    fn visit_str<E>(self, v: &str) -> std::result::Result<Mailbox, E>
    where
        E: de::Error,
    {
        v.parse()
            .map_err(|_| E::invalid_value(
                de::Unexpected::Str(v), &"an email address"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UseTls {
    /// Do not use TLS.
    No,
    /// Try to use TLS and fall back to unencrypted if TLS is not supported.
    Yes,
    /// Always use TLS.
    Strict,
}

impl Default for UseTls {
    fn default() -> Self {
        UseTls::Yes
    }
}

impl<'de> Deserialize<'de> for UseTls {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_bool(UseTlsVisitor)
    }
}

struct UseTlsVisitor;

impl<'de> de::Visitor<'de> for UseTlsVisitor {
    type Value = UseTls;

    fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "true, false, or strict")
    }

    fn visit_bool<E>(self, v: bool) -> Result<UseTls, E> {
        Ok(if v { UseTls::Yes } else { UseTls::No })
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<UseTls, E> {
        match v {
            "strict" | "always" => Ok(UseTls::Strict),
            _ => Err(E::invalid_value(
                de::Unexpected::Str(v), &"true, false, or strict")),
        }
    }
}
