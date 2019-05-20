//! Support framework.
//!
//! This module contains various utilities used by the test framework and
//! macros.

use failure::Error;

use super::db::{Database, Pool, Pooled};

/// Only types implementing this trait can be returned from test functions.
pub trait TestResult {
    /// Convert this value into a test result.
    fn into_result(self) -> Result<(), Error>;
}

impl<T, E> TestResult for Result<T, E>
where
    Error: From<E>,
{
    fn into_result(self) -> Result<(), Error> {
        self.map(|_| ()).map_err(From::from)
    }
}

impl TestResult for () {
    fn into_result(self) -> Result<(), Error> {
        Ok(self)
    }
}

/// Test configuration.
///
/// This structure is only used to provide [`Fixture::make`] with information it
/// might require for creating itself.
pub struct TestOptions<'a> {
    /// Pool of connections to a test database.
    pub pool: &'a Pool,
}

/// Common trait implemented by test fixtures.
///
/// Test functions can take arguments of types implementing this trait.
pub trait Fixture: Sized {
    fn make(opts: &TestOptions) -> Result<Self, Error>;
}

/// Common trait implemented by all tests.
pub trait Test<Args: Fixture> {
    /// Result of running this test.
    type Result: TestResult;

    /// Run the test.
    fn run(&self, args: Args) -> Self::Result;
}

/// Run a test case.
pub fn run_test<A, T>(db: &Database, test: T)
where
    A: Fixture,
    T: Test<A>,
{
    let _ = env_logger::builder().is_test(true).try_init();

    match db.lock(|pool| {
        let fixtures = A::make(&TestOptions {
            pool: &pool,
        })?;

        test.run(fixtures).into_result()
    }) {
        Ok(_) => (),
        Err(err) => panic!("{}", err),
    }
}

impl Fixture for () {
    fn make(_: &TestOptions) -> Result<Self, Error> {
        Ok(())
    }
}

impl Fixture for Pool {
    fn make(opts: &TestOptions) -> Result<Self, Error> {
        Ok(opts.pool.clone())
    }
}

impl Fixture for Pooled {
    fn make(opts: &TestOptions) -> Result<Self, Error> {
        opts.pool.get().map_err(From::from)
    }
}

impl<R, F> Test<()> for F
where
    R: TestResult,
    F: Fn() -> R,
{
    type Result = R;

    fn run(&self, (): ()) -> R {
        self()
    }
}

macro_rules! impl_test {
    {
        $( $($id:ident),+ );+ $(;)?
    } => {
        $(
            impl<$($id),+> Fixture for ($($id,)+)
            where
                $($id: Fixture,)+
            {
                fn make(opts: &TestOptions) -> Result<Self, Error> {
                    Ok((
                        $(<$id as Fixture>::make(opts)?,)+
                    ))
                }
            }

            impl<$($id,)+ R, Func> Test<($($id,)+)> for Func
            where
                $($id: Fixture,)+
                R: TestResult,
                Func: Fn($($id),+) -> R,
            {
                type Result = R;

                #[allow(non_snake_case)]
                fn run(&self, ($($id,)+): ($($id,)+)) -> Self::Result {
                    self($($id),+)
                }
            }
        )+
    }
}

impl_test! {
    A;
    A, B;
    A, B, C;
    A, B, C, D;
    A, B, C, D, E;
    A, B, C, D, E, F;
}
