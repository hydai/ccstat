//! Pi provider for ccstat
//!
//! This crate implements the provider trait for Pi,
//! handling JSONL session parsing with [pi] model prefix.

pub mod data_loader;

pub use data_loader::DataLoader;
