use diesel::pg::PgConnection;
use failure::err_msg;
use r2d2_diesel::ConnectionManager;
use std::env;

use crate::utils::SingleInit;
use super::Config;

pub mod functions;
pub mod models;
pub mod schema;
pub mod types;

pub type Connection = PgConnection;

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;

/// Find the correct database URL based on configuration and environment.
pub fn database_url(cfg: &Config) -> Result<String, GetDatabaseUrlError> {
    match env::var("DATABASE_URL") {
        Ok(url) => return Ok(url),
        Err(env::VarError::NotUnicode(_)) => {
            return Err(GetDatabaseUrlError::VarInvalidUnicode);
        }
        Err(env::VarError::NotPresent) => (),
    }

    if let Some(ref db) = cfg.database {
        return Ok(db.url.clone());
    }

    Err(GetDatabaseUrlError::NotConfigured)
}

#[derive(Debug, Fail)]
pub enum GetDatabaseUrlError {
    #[fail(display = "No database connection configured")]
    NotConfigured,
    #[fail(display = "DATABASE_URL contains invalid Unicode")]
    VarInvalidUnicode,
}

/// Create a new connection.
pub fn connect(cfg: &Config) -> crate::Result<Connection> {
    use diesel::Connection;

    let url = database_url(cfg)?;
    let conn = PgConnection::establish(&url)?;

    Ok(conn)
}

static POOL: SingleInit<Pool> = SingleInit::uninit();

/// Create a connection pool for the database.
///
/// Note that this function will only ever create a single pool. If it has
/// succeeded once, every call after that will return the same pool.
pub fn pool(cfg: &Config) -> crate::Result<Pool> {
    POOL.get_or_try_init(|| {
        let url = database_url(cfg)?;
        let manager = ConnectionManager::new(url);
        let pool = Pool::new(manager)?;

        // Try to connect to database to detect errors early.
        let conn = pool.get()?;

        // Run migrations in production build.
        if cfg!(not(debug_assertions)) {
            embedded_migrations::run_with_output(&*conn, &mut ::std::io::stderr())
                .map_err(|_| err_msg("Migrations failed"))?;
        }

        Ok(pool)
    }).map(Clone::clone)
}

// Embed migrations when building for production.
#[cfg(not(debug_assertions))]
embed_migrations!();

// `pool` requires embedded_migrations::run_with_output to typecheck, even
// if it's never used.
#[cfg(debug_assertions)]
mod embedded_migrations {
    use diesel::pg::PgConnection;
    pub fn run_with_output<W>(_: &PgConnection, _: &mut W) -> Result<(), ()> {
        Ok(())
    }
}
