//! Pricing fetcher module for LiteLLM model pricing data

use crate::error::Result;
use crate::types::ModelPricing;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// LiteLLM pricing API URL
const LITELLM_PRICING_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

/// Embedded pricing data for offline mode
const EMBEDDED_PRICING: &str = include_str!("../embedded/pricing.json");

/// Fetches and caches model pricing data
pub struct PricingFetcher {
    /// Cached pricing data
    cache: Arc<RwLock<Option<HashMap<String, ModelPricing>>>>,
    /// Whether to operate in offline mode
    offline_mode: bool,
    /// HTTP client
    client: reqwest::Client,
}

impl PricingFetcher {
    /// Create a new PricingFetcher
    pub async fn new(offline: bool) -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            offline_mode: offline,
            client: reqwest::Client::new(),
        }
    }

    /// Get pricing for a specific model
    pub async fn get_model_pricing(&self, model_name: &str) -> Result<Option<ModelPricing>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(ref pricing_map) = *cache {
                if let Some(pricing) = Self::find_model_pricing(pricing_map, model_name) {
                    return Ok(Some(pricing.clone()));
                }
            }
        }

        // Load pricing if not cached
        self.ensure_pricing_loaded().await?;

        // Check again after loading
        let cache = self.cache.read().await;
        Ok(cache
            .as_ref()
            .and_then(|map| Self::find_model_pricing(map, model_name))
            .cloned())
    }

    /// Ensure pricing data is loaded
    async fn ensure_pricing_loaded(&self) -> Result<()> {
        let mut cache = self.cache.write().await;
        if cache.is_some() {
            return Ok(());
        }

        let pricing_data = self.fetch_pricing_data().await?;
        *cache = Some(pricing_data);
        Ok(())
    }

    /// Fetch pricing data from LiteLLM or embedded data
    async fn fetch_pricing_data(&self) -> Result<HashMap<String, ModelPricing>> {
        if self.offline_mode {
            info!("Using embedded pricing data (offline mode)");
            return Self::parse_embedded_pricing();
        }

        match self.fetch_litellm_pricing().await {
            Ok(data) => {
                info!("Successfully fetched pricing data from LiteLLM");
                Ok(data)
            }
            Err(e) => {
                warn!("Failed to fetch pricing data: {}, using embedded data", e);
                Self::parse_embedded_pricing()
            }
        }
    }

    /// Fetch pricing from LiteLLM API
    async fn fetch_litellm_pricing(&self) -> Result<HashMap<String, ModelPricing>> {
        let response = self.client.get(LITELLM_PRICING_URL).send().await?;

        let data: HashMap<String, serde_json::Value> = response.json().await?;
        Ok(Self::parse_pricing_data(data))
    }

    /// Parse pricing data from JSON
    fn parse_pricing_data(
        data: HashMap<String, serde_json::Value>,
    ) -> HashMap<String, ModelPricing> {
        let mut pricing_map = HashMap::new();

        for (model_name, value) in data {
            if let Ok(pricing) = serde_json::from_value::<ModelPricing>(value) {
                pricing_map.insert(model_name, pricing);
            }
        }

        pricing_map
    }

    /// Load embedded pricing data
    fn parse_embedded_pricing() -> Result<HashMap<String, ModelPricing>> {
        let data: HashMap<String, serde_json::Value> = serde_json::from_str(EMBEDDED_PRICING)?;
        Ok(Self::parse_pricing_data(data))
    }

    /// Find pricing for a model, with fuzzy matching
    fn find_model_pricing<'a>(
        pricing_map: &'a HashMap<String, ModelPricing>,
        model_name: &'a str,
    ) -> Option<&'a ModelPricing> {
        // Exact match
        if let Some(pricing) = pricing_map.get(model_name) {
            return Some(pricing);
        }

        // Try common variations
        let variations = [
            model_name.to_string(),
            format!("anthropic/{model_name}"),
            format!("claude-{model_name}"),
            model_name.replace("claude-3-", "claude-3."),
            model_name.replace("claude-3.", "claude-3-"),
        ];

        for variant in &variations {
            if let Some(pricing) = pricing_map.get(variant) {
                debug!("Found pricing for {} using variant {}", model_name, variant);
                return Some(pricing);
            }
        }

        // Partial match (contains model name)
        for (key, pricing) in pricing_map {
            if key.contains(model_name) || model_name.contains(key) {
                debug!(
                    "Found pricing for {} using partial match {}",
                    model_name, key
                );
                return Some(pricing);
            }
        }

        None
    }

    /// Force refresh pricing data
    pub async fn refresh(&self) -> Result<()> {
        let mut cache = self.cache.write().await;
        *cache = None;
        drop(cache);

        self.ensure_pricing_loaded().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pricing_fetcher_creation() {
        let fetcher = PricingFetcher::new(true).await;
        assert!(fetcher.offline_mode);
    }

    #[test]
    fn test_model_name_variations() {
        let mut pricing_map = HashMap::new();
        pricing_map.insert(
            "claude-3-opus".to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.00001),
                output_cost_per_token: Some(0.00002),
                cache_creation_input_token_cost: None,
                cache_read_input_token_cost: None,
            },
        );

        // Should find exact match
        assert!(PricingFetcher::find_model_pricing(&pricing_map, "claude-3-opus").is_some());

        // Should find partial match
        assert!(PricingFetcher::find_model_pricing(&pricing_map, "opus").is_some());
    }
}
