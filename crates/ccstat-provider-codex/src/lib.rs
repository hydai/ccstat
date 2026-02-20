//! Codex provider for ccstat
//!
//! This crate implements the provider trait for OpenAI Codex,
//! handling JSONL session parsing with cumulative-to-delta token conversion.

pub mod data_loader;

pub use data_loader::DataLoader;
