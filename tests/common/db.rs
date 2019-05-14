//! Managing tests databases.

use adaptarr::impl_from;
use diesel::{
    RunQueryDsl,
    backend::Backend,
    connection::SimpleConnection,
    pg::PgConnection,
    query_builder::*,
    result::QueryResult,
};
use diesel_migrations::{
    MigrationError,
    RunMigrationsError,
    find_migrations_directory,
    run_pending_migrations_in_directory,
};
use failure::{Error, Fail};
use r2d2_diesel::ConnectionManager;
use std::sync::Mutex;

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;

pub struct Database {
    lock: Mutex<()>,
    pool: Pool,
    seed: Box<dyn Fn(&PgConnection) -> Result<(), Error> + Sync>,
}

impl Database {
    /// Obtain an exclusive lock to a test database.
    pub fn lock<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(Pool) -> Result<R, Error>,
    {
        // Ensure we have exclusive access to database.
        let _guard = match self.lock.lock() {
            Ok(guard) => guard,
            Err(poison) => poison.into_inner(),
        };

        // Clear and re-seed database.
        let conn = self.pool.get()?;
        conn.batch_execute(CLEAR_DATABASE)?;
        (self.seed)(&conn)?;

        f(self.pool.clone())
    }
}

/// Setup a new database for testing.
///
/// This function will create a new database, apply all migrations to it, and
/// initialize it with provided seed data.
///
/// You will most likely want to create a single database per integration test
/// suite. To do so you can use `lazy_static`:
///
/// ```ignore
/// lazy_static! {
///     static ref DATABSE: Pool = setup_db(|con| {
///         // Seed database
///     }).expect("Cannot create test database");
/// }
/// ```
pub fn setup_db<F>(seed: F) -> Result<Database, Error>
where
    F: Fn(&PgConnection) -> Result<(), Error> + Sync + 'static,
{
    let url = database_url();
    let create = std::env::var_os("TEST_DONT_CREATE_DATABASE").is_none();

    if create {
        // Create test database, dropping previous if needed.
        eprint!("Re-creating database. Set TEST_DONT_CREATE_DATABASE to skip");
        let (database, default_url) = change_database_of_url(&url);
        let conn = PgConnection::establish(&default_url)?;
        drop_database(&database).if_exists().execute(&conn)?;
        create_database(&database).execute(&conn)?;
    }

    // Connect to test database
    let conn = PgConnection::establish(&url)?;

    if create {
        // Run migrations
        let migrations_dir = find_migrations_directory()?;
        run_pending_migrations_in_directory(
            &conn, &migrations_dir, &mut ::std::io::stderr())?;
    }

    // Finished
    Ok(Database {
        lock: Mutex::new(()),
        pool: Pool::new(ConnectionManager::new(url))?,
        seed: Box::new(seed),
    })
}

#[derive(Debug, Fail)]
pub enum SetupDbError {
    #[fail(display = "{}", _0)]
    Database(#[cause] diesel::result::Error),
    #[fail(display = "{}", _0)]
    Connection(#[cause] diesel::ConnectionError),
    #[fail(display = "{}", _0)]
    Pool(#[cause] r2d2::Error),
    #[fail(display = "{}", _0)]
    Migration(#[cause] RunMigrationsError),
}

impl_from! { for SetupDbError ;
    diesel::result::Error => |e| SetupDbError::Database(e),
    diesel::ConnectionError => |e| SetupDbError::Connection(e),
    r2d2::Error => |e| SetupDbError::Pool(e),
    RunMigrationsError => |e| SetupDbError::Migration(e),
    MigrationError => |e| SetupDbError::Migration(
        RunMigrationsError::MigrationError(e)),
}

/// Find correct database URL for testing.
fn database_url() -> String {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return url;
    }

    let mut url = "postgres://".to_string();

    if let Ok(user) = std::env::var("DATABASE_USER") {
        url.push_str(&user);
    }

    url.push('/');
    if let Ok(name) = std::env::var("DATABASE_NAME") {
        url.push_str(&name);
    } else {
        url.push_str("adaptarr-test");
    }

    url
}

/// Change connection URL to point to the default database. Return it and name
/// of the original database.
///
/// Taken from `diesel-cli`.
fn change_database_of_url(url: &str) -> (String, String) {
    let base = ::url::Url::parse(url).unwrap();
    let database = base.path_segments().unwrap().last().unwrap().to_owned();
    let mut new_url = base.join("postgres").unwrap();
    new_url.set_query(base.query());
    (database, new_url.into_string())
}

const CLEAR_DATABASE: &str = r#"
do $$
declare
    stmt text;
begin
    select 'TRUNCATE '
        || string_agg(format('%I.%I', schemaname, tablename), ', ')
    into stmt
    from pg_tables
    where schemaname = 'public'
      and tablename not like '__diesel_%';

    execute stmt;

    for stmt in (
        select 'alter sequence ' || relname || ' restart with 1;'
        from pg_class
        where relkind = 'S'
    ) loop
        execute stmt;
    end loop;
end; $$
"#;

// -----------------------------------------------------------------------------
// Based on Diesel's `diesel_cli/src/query_helper.rs`.

#[derive(Debug, Clone)]
pub struct DropDatabaseStatement<'a> {
    name: &'a str,
    if_exists: bool,
}

impl<'a> DropDatabaseStatement<'a> {
    pub fn new(name: &'a str) -> Self {
        DropDatabaseStatement {
            name,
            if_exists: false,
        }
    }

    pub fn if_exists(self) -> Self {
        DropDatabaseStatement {
            if_exists: true,
            ..self
        }
    }
}

impl<'a, DB: Backend> QueryFragment<DB> for DropDatabaseStatement<'a> {
    fn walk_ast(&self, mut out: AstPass<DB>) -> QueryResult<()> {
        out.push_sql("DROP DATABASE ");
        if self.if_exists {
            out.push_sql("IF EXISTS ");
        }
        out.push_identifier(self.name)?;
        Ok(())
    }
}

impl<'a, Conn> RunQueryDsl<Conn> for DropDatabaseStatement<'a> {}

impl<'a> QueryId for DropDatabaseStatement<'a> {
    type QueryId = ();

    const HAS_STATIC_QUERY_ID: bool = false;
}

#[derive(Debug, Clone)]
pub struct CreateDatabaseStatement<'a> {
    name: &'a str,
    template: Option<&'a str>,
}

impl<'a> CreateDatabaseStatement<'a> {
    pub fn new(name: &'a str) -> Self {
        CreateDatabaseStatement {
            name,
            template: None,
        }
    }
}

impl<'a, DB: Backend> QueryFragment<DB> for CreateDatabaseStatement<'a> {
    fn walk_ast(&self, mut out: AstPass<DB>) -> QueryResult<()> {
        out.push_sql("CREATE DATABASE ");
        out.push_identifier(self.name)?;

        if let Some(template) = self.template {
            out.push_sql(" WITH TEMPLATE ");
            out.push_identifier(template)?;
        }

        Ok(())
    }
}

impl<'a, Conn> RunQueryDsl<Conn> for CreateDatabaseStatement<'a> {}

impl<'a> QueryId for CreateDatabaseStatement<'a> {
    type QueryId = ();

    const HAS_STATIC_QUERY_ID: bool = false;
}

pub fn drop_database(name: &str) -> DropDatabaseStatement {
    DropDatabaseStatement::new(name)
}

pub fn create_database(name: &str) -> CreateDatabaseStatement {
    CreateDatabaseStatement::new(name)
}
