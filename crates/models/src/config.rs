use adaptarr_util::SingleInit;
use serde::Deserialize;

use crate::db::Config as DbConfig;

static CONFIG: SingleInit<&'static Config> = SingleInit::uninit();

static DOMAIN: SingleInit<String> = SingleInit::uninit();

static SECRET: SingleInit<Box<[u8]>> = SingleInit::uninit();

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub database: Option<DbConfig>,
    pub storage: Storage,
}

/// File storage configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Storage {
    /// Path to a directory in which user-uploaded files will be kept.
    pub path: std::path::PathBuf,
}

impl Config {
    /// Get global configuration.
    ///
    /// ## Panics
    ///
    /// This function will panic if called before [`Config::register`].
    pub fn global() -> &'static Config {
        CONFIG.get().expect("model configuration must be initialized before \
            calling Config::global")
    }

    /// Get configured domain.
    ///
    /// ## Panics
    ///
    /// This function will panic if called before [`Config::register`].
    pub fn domain() -> &'static str {
        DOMAIN.get().expect("model configuration must be initialized before \
            calling Config::domain")
    }

    /// Get configured secret.
    ///
    /// ## Panics
    ///
    /// This function will panic if called before [`Config::register`].
    pub fn secret() -> &'static [u8] {
        SECRET.get().expect("model configuration must be initialized before \
            calling Config::secret")
    }

    /// Register this configuration as the global static configuration
    /// ([`Config::global`]).
    pub fn register(&'static self, domain: &str, secret: &[u8]) {
        CONFIG.get_or_init(|| self);
        DOMAIN.get_or_init(|| domain.to_string());
        SECRET.get_or_init(|| secret.to_vec().into_boxed_slice());
    }
}
