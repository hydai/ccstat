//! Amp provider for ccstat
//!
//! This crate implements the provider trait for Amp,
//! handling thread-based JSON and usageLedger events.

pub mod data_loader;

pub use data_loader::DataLoader;
