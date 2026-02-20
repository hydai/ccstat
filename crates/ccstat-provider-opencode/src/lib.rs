//! OpenCode provider for ccstat
//!
//! This crate implements the provider trait for OpenCode,
//! handling per-message JSON files and session metadata.

pub mod data_loader;

pub use data_loader::DataLoader;
