## Running tests

Adaptarr! uses standard Rust testing framework with custom extensions, and
slightly changed (compared to convention) test structure. Notably, many unit
tests are written as integration tests instead, since it allows for easier test
database management.

To run tests (unit, integration, and doc-tests) simply use cargo

```sh
cargo test
```

To run only unit tests, pass `--lib`, to run only tests from a particular
integration test suite, pass `--test suite-name`, and to run only doc-tests,
pass `--doc`. To only run tests matching a pattern, simply pass that pattern as
the last argument.
