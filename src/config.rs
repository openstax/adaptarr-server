use failure::Fail;
use log::LevelFilter;
use rand::RngCore;
use std::{collections::HashMap, fmt, fs, net::{SocketAddr, Ipv4Addr}};
use toml;
use serde::{Deserialize, de::{Deserializer, Error, Visitor, Unexpected}};

use crate::utils::SingleInit;

static CONFIG: SingleInit<Config> = SingleInit::uninit();

pub fn load() -> crate::Result<&'static Config> {
    CONFIG.get_or_try_init(|| {
        let data = fs::read("config.toml").map_err(ReadConfigurationError)?;
        toml::from_slice(&data).map_err(|e| ConfigurationError(e).into())
    })
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub server: Server,
    pub mail: crate::mail::Config,
    #[serde(default)]
    pub logging: Logging,
    pub sentry: Option<Sentry>,
}

impl Config {
    /// Validate configuration correctness.
    pub fn validate(&self) -> Result<(), failure::Error> {
        self.mail.validate()?;

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Server {
    /// Address on which to listen.
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    /// Domain (host name) of this server.
    pub domain: String,
    /// Secret key.
    #[serde(default = "random_secret", deserialize_with = "de_secret")]
    pub secret: Vec<u8>,
}

/// Logging configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Logging {
    /// Default logging level.
    #[serde(default = "default_level_filter")]
    pub level: LevelFilter,
    /// Actix-web logging level.
    pub network: Option<LevelFilter>,
    /// Custom filters.
    #[serde(default)]
    pub filters: HashMap<String, LevelFilter>,
}

/// Sentry.io configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Sentry {
    /// Client key.
    pub dsn: String,
}

#[derive(Debug, Fail)]
#[fail(display = "Cannot read configuration file")]
pub struct ReadConfigurationError(#[fail(cause)] std::io::Error);

#[derive(Debug, Fail)]
#[fail(display = "Invalid configuration: {}", _0)]
pub struct ConfigurationError(#[fail(cause)] toml::de::Error);

/// Default address (127.0.0.1:80).
fn default_address() -> SocketAddr {
    (Ipv4Addr::LOCALHOST, 80).into()
}

/// Default secret (32 random bytes).
fn random_secret() -> Vec<u8> {
    let mut secret = vec![0; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    secret
}

/// Deserialize a secret key.
fn de_secret<'de, D>(d: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    d.deserialize_byte_buf(SecretVisitor)
}

struct SecretVisitor;

impl<'de> Visitor<'de> for SecretVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "a binary data or a file")
    }

    fn visit_str<E>(self, v: &str) -> Result<Vec<u8>, E>
    where
        E: Error,
    {
        if v.starts_with("base64:") {
            base64::decode(v.trim_start_matches("base64:"))
                .map_err(E::custom)
                .and_then(|v| self.visit_byte_buf(v))
        } else if v.starts_with("file:") {
            fs::read(v.trim_start_matches("file:"))
                .map_err(E::custom)
                .and_then(|v| self.visit_byte_buf(v))
        } else {
            Err(E::invalid_value(
                Unexpected::Str(v), &"an encoded binary string or a file"))
        }
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Vec<u8>, E>
    where
        E: Error,
    {
        if v.len() < 32 {
            return Err(E::invalid_length(v.len(), &"at least 32 bytes"));
        }
        Ok(v)
    }
}

fn default_level_filter() -> LevelFilter {
    LevelFilter::Info
}

impl Default for Logging {
    fn default() -> Self {
        Logging {
            level: default_level_filter(),
            network: None,
            filters: HashMap::new(),
        }
    }
}
