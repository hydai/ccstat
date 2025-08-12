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
            if let Some(ref pricing_map) = *cache
                && let Some(pricing) = Self::find_model_pricing(pricing_map, model_name)
            {
                return Ok(Some(pricing.clone()));
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

        let online_fetcher = PricingFetcher::new(false).await;
        assert!(!online_fetcher.offline_mode);
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

    #[tokio::test]
    async fn test_get_model_pricing_offline() {
        let fetcher = PricingFetcher::new(true).await;

        // Test getting pricing for a known model
        let pricing = fetcher
            .get_model_pricing("claude-3-opus-20240229")
            .await
            .unwrap();
        assert!(pricing.is_some());

        let pricing = pricing.unwrap();
        assert!(pricing.input_cost_per_token.is_some());
        assert!(pricing.output_cost_per_token.is_some());
    }

    #[tokio::test]
    async fn test_get_model_pricing_unknown_model() {
        let fetcher = PricingFetcher::new(true).await;

        // Test getting pricing for an unknown model
        let pricing = fetcher
            .get_model_pricing("unknown-model-xyz")
            .await
            .unwrap();
        assert!(pricing.is_none());
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        let fetcher = PricingFetcher::new(true).await;

        // First call should load the cache
        let pricing1 = fetcher
            .get_model_pricing("claude-3-opus-20240229")
            .await
            .unwrap();
        assert!(pricing1.is_some());

        // Second call should use cached data
        let pricing2 = fetcher
            .get_model_pricing("claude-3-opus-20240229")
            .await
            .unwrap();
        assert!(pricing2.is_some());

        // Both should be the same
        assert_eq!(pricing1, pricing2);
    }

    #[tokio::test]
    async fn test_refresh_cache() {
        let fetcher = PricingFetcher::new(true).await;

        // Load initial pricing
        let pricing1 = fetcher
            .get_model_pricing("claude-3-opus-20240229")
            .await
            .unwrap();
        assert!(pricing1.is_some());

        // Refresh the cache
        fetcher.refresh().await.unwrap();

        // Should still get pricing after refresh
        let pricing2 = fetcher
            .get_model_pricing("claude-3-opus-20240229")
            .await
            .unwrap();
        assert!(pricing2.is_some());
    }

    #[test]
    fn test_parse_embedded_pricing() {
        let result = PricingFetcher::parse_embedded_pricing();
        assert!(result.is_ok());

        let pricing_map = result.unwrap();
        assert!(!pricing_map.is_empty());

        // Check that some known models exist (using actual model names from embedded data)
        let known_models = vec![
            "claude-3-opus",
            "claude-3-sonnet",
            "claude-3-haiku",
            "claude-3.5-sonnet",
        ];

        for model in known_models {
            assert!(pricing_map.contains_key(model), "Missing model: {}", model);
        }
    }

    #[test]
    fn test_model_variations_matching() {
        let mut pricing_map = HashMap::new();

        // Add different model name formats
        pricing_map.insert(
            "anthropic/claude-3-opus".to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.00001),
                output_cost_per_token: Some(0.00002),
                cache_creation_input_token_cost: None,
                cache_read_input_token_cost: None,
            },
        );

        pricing_map.insert(
            "claude-3.5-sonnet".to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.00001),
                output_cost_per_token: Some(0.00002),
                cache_creation_input_token_cost: None,
                cache_read_input_token_cost: None,
            },
        );

        // Test various matching patterns
        assert!(PricingFetcher::find_model_pricing(&pricing_map, "claude-3-opus").is_some());
        assert!(PricingFetcher::find_model_pricing(&pricing_map, "claude-3.5-sonnet").is_some());
        assert!(PricingFetcher::find_model_pricing(&pricing_map, "claude-3-5-sonnet").is_some());
    }

    #[test]
    fn test_parse_pricing_data() {
        let mut data = HashMap::new();

        // Valid pricing data
        data.insert(
            "model1".to_string(),
            serde_json::json!({
                "input_cost_per_token": 0.00001,
                "output_cost_per_token": 0.00002,
                "cache_creation_input_token_cost": 0.000015,
                "cache_read_input_token_cost": 0.000001
            }),
        );

        // Invalid data (should be skipped)
        data.insert(
            "model2".to_string(),
            serde_json::json!({
                "invalid_field": "test"
            }),
        );

        let pricing_map = PricingFetcher::parse_pricing_data(data);

        // Should have model1, model2 might parse with default values
        assert!(pricing_map.contains_key("model1"));

        let model1_pricing = &pricing_map["model1"];
        assert_eq!(model1_pricing.input_cost_per_token, Some(0.00001));
        assert_eq!(model1_pricing.output_cost_per_token, Some(0.00002));
        assert_eq!(
            model1_pricing.cache_creation_input_token_cost,
            Some(0.000015)
        );
        assert_eq!(model1_pricing.cache_read_input_token_cost, Some(0.000001));

        // Check that model2 is parsed with None values for invalid fields
        assert!(pricing_map.contains_key("model2"));
        let model2_pricing = pricing_map
            .get("model2")
            .expect("model2 should be in the map");
        assert!(model2_pricing.input_cost_per_token.is_none());
        assert!(model2_pricing.output_cost_per_token.is_none());
        assert!(model2_pricing.cache_creation_input_token_cost.is_none());
        assert!(model2_pricing.cache_read_input_token_cost.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_cache_access() {
        use std::sync::Arc;
        use tokio::task;

        let fetcher = Arc::new(PricingFetcher::new(true).await);

        // Spawn multiple tasks accessing the cache concurrently
        let mut handles = vec![];

        for _ in 0..10 {
            let fetcher_clone = fetcher.clone();
            let handle = task::spawn(async move {
                fetcher_clone
                    .get_model_pricing("claude-3-opus-20240229")
                    .await
            });
            handles.push(handle);
        }

        // All tasks should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
            assert!(result.unwrap().is_some());
        }
    }

    #[test]
    fn test_partial_matching_priority() {
        let mut pricing_map = HashMap::new();

        // Add models with overlapping names
        pricing_map.insert(
            "claude-3-opus".to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.00001),
                output_cost_per_token: Some(0.00002),
                cache_creation_input_token_cost: None,
                cache_read_input_token_cost: None,
            },
        );

        pricing_map.insert(
            "claude-3-opus-20240229".to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.00003),
                output_cost_per_token: Some(0.00004),
                cache_creation_input_token_cost: None,
                cache_read_input_token_cost: None,
            },
        );

        // Exact match should take priority
        let pricing = PricingFetcher::find_model_pricing(&pricing_map, "claude-3-opus-20240229");
        assert!(pricing.is_some());
        assert_eq!(pricing.unwrap().input_cost_per_token, Some(0.00003));

        // Exact match for shorter name
        let pricing = PricingFetcher::find_model_pricing(&pricing_map, "claude-3-opus");
        assert!(pricing.is_some());
        assert_eq!(pricing.unwrap().input_cost_per_token, Some(0.00001));
    }
}
