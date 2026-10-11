//! Single integration-test binary. Each module is one test file listed in tasks.md.
//! Tests never touch live platforms; they use `tests/fixtures/` and fake tool runners.

#[macro_use]
mod common;

mod contract_selftest;

mod db;
mod dedup;
mod net;
mod sanitize;
mod secrets;
