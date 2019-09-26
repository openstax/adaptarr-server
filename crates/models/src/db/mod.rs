use adaptarr_macros::From;
use adaptarr_util::SingleInit;
use diesel::pg::PgConnection;
use failure::Fail;
use r2d2_diesel::ConnectionManager;
use serde::Deserialize;
use std::env;

pub mod functions;
pub mod models;
pub mod schema;
pub mod types;

/// A single connection to a database.
pub type Connection = PgConnection;

/// A pool of database connections.
pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;

/// A single database connection taken from a [`Pool`] of connection.
pub type PooledConnection = r2d2::PooledConnection<ConnectionManager<PgConnection>>;

/// Database configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub url: String,
}

/// Find the correct database URL based on configuration and environment.
pub fn database_url(cfg: Option<&Config>) -> Result<String, GetDatabaseUrlError> {
    match env::var("DATABASE_URL") {
        Ok(url) => return Ok(url),
        Err(env::VarError::NotUnicode(_)) => {
            return Err(GetDatabaseUrlError::VarInvalidUnicode);
        }
        Err(env::VarError::NotPresent) => (),
    }

    if let Some(ref db) = cfg {
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
pub fn connect(cfg: Option<&Config>) -> Result<Connection, ConnectionError> {
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
pub fn configure_pool(cfg: Option<&Config>) -> Result<Pool, ConnectionError> {
    POOL.get_or_try_init(|| {
        let url = database_url(cfg)?;
        let manager = ConnectionManager::new(url);
        let pool = Pool::new(manager)?;

        // Try to connect to database to detect errors early.
        let conn = pool.get()?;

        // Run migrations in production build.
        if cfg!(not(debug_assertions)) {
            embedded_migrations::run_with_output(&*conn, &mut ::std::io::stderr())
                .map_err(ConnectionError::Migration)?;
        }

        Ok(pool)
    }).map(Clone::clone)
}

/// Get a pool of database connections.
///
/// ## Panics
///
/// This function will panic when called before [`configure_pool`].
pub fn pool() -> Pool {
    POOL.get().expect("uninitialized database").clone()
}

#[derive(Debug, Fail, From)]
pub enum ConnectionError {
    #[fail(display = "{}", _0)]
    Configuration(#[cause] #[from] GetDatabaseUrlError),
    #[fail(display = "{}", _0)]
    Pool(#[from] r2d2::Error),
    #[fail(display = "{}", _0)]
    Database(#[from] diesel::ConnectionError),
    #[cfg(debug_assertions)]
    #[fail(display = "could not perform migrations")]
    Migration(()),
    #[cfg(not(debug_assertions))]
    #[fail(display = "{}", _0)]
    Migration(diesel_migrations::RunMigrationsError),
}

// Embed migrations when building for production.
#[cfg(not(debug_assertions))]
diesel_migrations::embed_migrations!();

// `pool` requires embedded_migrations::run_with_output to typecheck, even
// if it's never used.
#[cfg(debug_assertions)]
mod embedded_migrations {
    use diesel::pg::PgConnection;
    pub fn run_with_output<W>(_: &PgConnection, _: &mut W) -> Result<(), ()> {
        Ok(())
    }
}
