//! Library root — exposes all modules so integration tests and the binary can share them.

pub mod api;
pub mod cache;
pub mod clickhouse;
pub mod config;
pub mod error;
pub mod pivot;
pub mod query;
pub mod semantic;
pub mod state;
