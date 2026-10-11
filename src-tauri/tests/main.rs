//! Single integration-test binary. Each module is one test file listed in tasks.md.
//! Tests never touch live platforms; they use `tests/fixtures/` and fake tool runners.

#[macro_use]
mod common;

mod contract_selftest;
mod contract_soundcloud;
mod contract_spotify;
mod contract_youtube;

mod db;
mod dedup;
mod finalize;
mod importer;
mod matcher;
mod matcher_accuracy;
mod net;
mod oauth;
mod queue;
mod sanitize;
mod secrets;
mod tagger;
