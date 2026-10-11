//! IPC layer only (contracts/ipc.md). Commands validate input, call into `core/` and map errors.

pub mod accounts;
pub mod import;
pub mod jobs;
pub mod library;
pub mod settings;
pub mod tools;
