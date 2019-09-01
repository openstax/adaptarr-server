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
