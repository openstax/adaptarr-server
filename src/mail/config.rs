use serde::{Deserialize, Deserializer};
use lettre_email::Mailbox;

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
    /// Validate configuration correctness.
    pub fn validate(&self) -> Result<(), failure::Error> {
        super::transport::from_config(self)?;
        Ok(())
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
    #[serde(default = "true_value")]
    pub use_tls: bool,
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

fn true_value() -> bool {
    true
}
