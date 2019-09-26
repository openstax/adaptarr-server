use adaptarr_models::Config as ModelConfig;
use adaptarr_util::SingleInit;
use failure::Fail;
use log::LevelFilter;
use serde::Deserialize;
use std::{collections::HashMap, fs};
use toml;

use crate::Result;

static CONFIG: SingleInit<Config> = SingleInit::uninit();

pub fn load() -> Result<&'static Config> {
    CONFIG.get_or_try_init(|| {
        let data = fs::read("config.toml").map_err(ReadConfigurationError)?;
        toml::from_slice(&data).map_err(|e| ConfigurationError(e).into())
    })
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub server: adaptarr_rest_api::Config,
    pub mail: adaptarr_mail::Config,
    #[serde(default)]
    pub logging: Logging,
    pub sentry: Option<Sentry>,
    #[serde(flatten)]
    pub model: ModelConfig,
}

impl Config {
    /// Validate configuration correctness.
    pub fn validate(&self) -> Result<(), failure::Error> {
        self.mail.validate()?;

        Ok(())
    }

    /// Register this configuration as the global static configuration
    /// ([`Config::global`]).
    pub fn register(&'static self) {
        self.mail.register();
        self.model.register(&self.server.domain, &self.server.secret);
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
