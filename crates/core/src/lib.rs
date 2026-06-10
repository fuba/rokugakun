//! Shared core for the game recording launcher.
//!
//! Maps spec.md `common/*` + `storage/*` (C++ §26) onto Rust modules. Pure data,
//! config, the SQLite manifest, and retention live here so both `launcher` and
//! `recorder` depend on one source of truth.

pub mod config;
pub mod domain;
pub mod error;
pub mod fsutil;
pub mod logging;
pub mod preset;
pub mod protocol;
pub mod retention;
pub mod store;
pub mod timebase;

pub use error::{Error, Result};
