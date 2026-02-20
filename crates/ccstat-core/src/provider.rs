//! Provider trait for data loaders
//!
//! This module defines the `ProviderDataLoader` trait that all provider crates
//! must implement. It provides a uniform interface for constructing a data loader
//! and streaming usage entries.

use crate::error::Result;
use crate::types::UsageEntry;
use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

/// Trait for provider-specific data loaders.
///
/// Each provider crate (Claude, Codex, OpenCode, Amp, Pi) implements this trait
/// so that the main binary can dispatch to any provider using generic code.
#[async_trait]
pub trait ProviderDataLoader: Send + Sync + Sized {
    /// Create a new data loader, discovering data directories.
    async fn new() -> Result<Self>;

    /// Stream all usage entries from the provider's data files.
    fn load_entries(&self) -> Pin<Box<dyn Stream<Item = Result<UsageEntry>> + Send + '_>>;
}
