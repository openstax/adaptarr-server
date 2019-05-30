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

/// Mail transport configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum Transports {
    /// Log messages to standard error.
    Log,
    /// Use the `sendmail(1)` command.
    Sendmail,
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
