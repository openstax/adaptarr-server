//! Server administration.

use crate::{Config, Result, api};

pub fn start(config: Config) -> Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    api::start(config)?;

    Ok(())
}
