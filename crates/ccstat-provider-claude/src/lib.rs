//! Claude Code provider for ccstat
//!
//! This crate implements the provider trait for Claude Code,
//! handling JSONL file discovery, parsing, and usage entry extraction.

pub mod data_loader;

#[cfg(test)]
pub mod test_utils;

pub use data_loader::DataLoader;
