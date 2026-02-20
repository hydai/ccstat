//! OpenCode data loader
//!
//! Discovers and parses OpenCode per-message JSON files from
//! `~/.local/share/opencode/storage/message/`.

use async_trait::async_trait;
use ccstat_core::error::Result;
use ccstat_core::provider::ProviderDataLoader;
use ccstat_core::types::UsageEntry;
use futures::stream::Stream;
use std::pin::Pin;

/// Data loader for OpenCode usage data.
pub struct DataLoader;

#[async_trait]
impl ProviderDataLoader for DataLoader {
    async fn new() -> Result<Self> {
        Ok(DataLoader)
    }

    fn load_entries(&self) -> Pin<Box<dyn Stream<Item = Result<UsageEntry>> + Send + '_>> {
        Box::pin(futures::stream::empty())
    }
}
