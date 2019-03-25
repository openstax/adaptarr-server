//! Server administration.

use crate::{Config, Result, api};

pub fn start(config: Config) -> Result<()> {
    api::start(config)?;

    Ok(())
}
