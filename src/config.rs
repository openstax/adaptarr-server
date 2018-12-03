use std::{fs, net::{SocketAddr, Ipv4Addr}};
use toml;

use super::Result;

pub fn load() -> Result<Config> {
    let data = fs::read("config.toml")?;
    toml::from_slice(&data).map_err(|e| ConfigurationError(e).into())
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub server: Server,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Server {
    /// Address on which to listen.
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    /// Domain (host name) of this server.
    pub domain: String,
}

#[derive(Debug, Fail)]
#[fail(display = "Invalid configuration: {}", _0)]
pub struct ConfigurationError(#[fail(cause)] toml::de::Error);

/// Default address (127.0.0.1:80).
fn default_address() -> SocketAddr {
    (Ipv4Addr::LOCALHOST, 80).into()
}
