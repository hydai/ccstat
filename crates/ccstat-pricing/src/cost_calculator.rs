//! Cost calculator module for computing usage costs
//!
//! This module provides the core cost calculation functionality, supporting
//! both pre-calculated costs and dynamic calculation based on LiteLLM pricing data.
//!
//! # Examples
//!
//! ```no_run
//! use ccstat_pricing::{
//!     cost_calculator::CostCalculator,
//!     pricing_fetcher::PricingFetcher,
//! };
//! use ccstat_core::types::{CostMode, ModelName, TokenCounts};
//! use std::sync::Arc;
//!
//! # async fn example() -> ccstat_core::Result<()> {
//! let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
//! let calculator = CostCalculator::new(pricing_fetcher);
//!
//! let tokens = TokenCounts::new(1000, 500, 100, 50);
//! let model = ModelName::new("claude-3-opus");
//!
//! // Calculate cost directly
//! let cost = calculator.calculate_cost(&tokens, &model).await?;
//!
//! // Calculate with mode consideration
//! let cost_with_mode = calculator
//!     .calculate_with_mode(&tokens, &model, Some(0.05), CostMode::Auto)
//!     .await?;
//! # Ok(())
//! # }
//! ```

use crate::pricing_fetcher::PricingFetcher;
use ccstat_core::error::{CcstatError, Result};
use ccstat_core::types::{CostMode, ModelName, ModelPricing, TokenCounts};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Calculates costs based on token usage and pricing
///
/// The CostCalculator integrates with the PricingFetcher to provide accurate
/// cost calculations for various Claude models. It supports multiple cost modes
/// allowing flexibility in how costs are computed.
pub struct CostCalculator {
    /// Pricing fetcher instance
    pricing_fetcher: Arc<PricingFetcher>,
    /// Tracks models already warned about to avoid duplicate warnings
    warned_models: Mutex<HashSet<String>>,
}

impl CostCalculator {
    /// Create a new CostCalculator with a pricing fetcher
    ///
    /// # Arguments
    ///
    /// * `pricing_fetcher` - Arc to a PricingFetcher instance for retrieving model pricing
    pub fn new(pricing_fetcher: Arc<PricingFetcher>) -> Self {
        Self {
            pricing_fetcher,
            warned_models: Mutex::new(HashSet::new()),
        }
    }

    /// Calculate cost for token usage
    ///
    /// Fetches the current pricing for the specified model and calculates
    /// the total cost based on token counts.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Token counts to calculate cost for
    /// * `model_name` - Name of the model to get pricing for
    ///
    /// # Errors
    ///
    /// Returns an error if the model is unknown or pricing data is unavailable
    pub async fn calculate_cost(
        &self,
        tokens: &TokenCounts,
        model_name: &ModelName,
    ) -> Result<f64> {
        let pricing = self
            .pricing_fetcher
            .get_model_pricing(model_name.as_str())
            .await?
            .ok_or_else(|| CcstatError::UnknownModel(model_name.clone()))?;

        Ok(Self::calculate_from_pricing(tokens, &pricing))
    }

    /// Calculate cost from pricing data without fetching
    ///
    /// This is a pure function that calculates cost given token counts and pricing.
    /// Useful when you already have pricing data and don't need to fetch it.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Token counts to calculate cost for
    /// * `pricing` - Model pricing information
    ///
    /// # Returns
    ///
    /// Total cost in dollars
    pub fn calculate_from_pricing(tokens: &TokenCounts, pricing: &ModelPricing) -> f64 {
        let mut cost = 0.0;

        if let Some(rate) = pricing.input_cost_per_token {
            cost += tokens.input_tokens as f64 * rate;
        }

        if let Some(rate) = pricing.output_cost_per_token {
            cost += tokens.output_tokens as f64 * rate;
        }

        if let Some(rate) = pricing.cache_creation_input_token_cost {
            cost += tokens.cache_creation_tokens as f64 * rate;
        }

        if let Some(rate) = pricing.cache_read_input_token_cost {
            cost += tokens.cache_read_tokens as f64 * rate;
        }

        debug!(
            "Calculated cost: ${:.6} for {} total tokens",
            cost,
            tokens.total()
        );

        cost
    }

    /// Calculate cost with mode consideration
    ///
    /// This method supports different cost calculation modes:
    /// - `Auto`: Uses pre-calculated cost if available, otherwise calculates
    /// - `Calculate`: Always calculates cost from tokens and pricing
    /// - `Display`: Only uses pre-calculated cost, errors if not available
    ///
    /// # Arguments
    ///
    /// * `tokens` - Token counts to calculate cost for
    /// * `model_name` - Name of the model to get pricing for
    /// * `pre_calculated` - Optional pre-calculated cost from usage data
    /// * `mode` - Cost calculation mode to use
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Model is unknown (in Calculate/Auto modes)
    /// - No pre-calculated cost is available (in Display mode)
    pub async fn calculate_with_mode(
        &self,
        tokens: &TokenCounts,
        model_name: &ModelName,
        pre_calculated: Option<f64>,
        mode: CostMode,
    ) -> Result<f64> {
        match mode {
            CostMode::Auto => {
                if let Some(cost) = pre_calculated {
                    Ok(cost)
                } else {
                    match self.calculate_cost(tokens, model_name).await {
                        Ok(cost) => Ok(cost),
                        Err(CcstatError::UnknownModel(ref name)) => {
                            if self.warned_models.lock().unwrap().insert(name.to_string()) {
                                warn!(model = %name, "Unknown model encountered, cost will be reported as $0.00");
                            }
                            Ok(0.0)
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            CostMode::Calculate => self.calculate_cost(tokens, model_name).await,
            CostMode::Display => pre_calculated.ok_or_else(|| {
                CcstatError::InvalidArgument(
                    "No pre-calculated cost available in display mode".to_string(),
                )
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_calculation() {
        let tokens = TokenCounts::new(1000, 500, 100, 50);
        let pricing = ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00002),
            cache_creation_input_token_cost: Some(0.000015),
            cache_read_input_token_cost: Some(0.000001),
        };

        let cost = CostCalculator::calculate_from_pricing(&tokens, &pricing);

        // Expected: (1000 * 0.00001) + (500 * 0.00002) + (100 * 0.000015) + (50 * 0.000001)
        // = 0.01 + 0.01 + 0.0015 + 0.00005 = 0.02155
        assert!((cost - 0.02155).abs() < 0.000001);
    }

    #[test]
    fn test_cost_with_missing_rates() {
        let tokens = TokenCounts::new(1000, 500, 100, 50);
        let pricing = ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00002),
            cache_creation_input_token_cost: None,
            cache_read_input_token_cost: None,
        };

        let cost = CostCalculator::calculate_from_pricing(&tokens, &pricing);

        // Expected: (1000 * 0.00001) + (500 * 0.00002) = 0.01 + 0.01 = 0.02
        assert!((cost - 0.02).abs() < 0.000001);
    }

    #[test]
    fn test_zero_tokens() {
        let tokens = TokenCounts::new(0, 0, 0, 0);
        let pricing = ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00002),
            cache_creation_input_token_cost: Some(0.000015),
            cache_read_input_token_cost: Some(0.000001),
        };

        let cost = CostCalculator::calculate_from_pricing(&tokens, &pricing);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_very_large_token_counts() {
        // Test with large but realistic token counts
        let tokens = TokenCounts::new(10_000_000, 5_000_000, 1_000_000, 500_000);
        let pricing = ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00002),
            cache_creation_input_token_cost: Some(0.000015),
            cache_read_input_token_cost: Some(0.000001),
        };

        let cost = CostCalculator::calculate_from_pricing(&tokens, &pricing);
        // Expected: (10M * 0.00001) + (5M * 0.00002) + (1M * 0.000015) + (500k * 0.000001)
        // = 100 + 100 + 15 + 0.5 = 215.5
        assert!((cost - 215.5).abs() < 0.01);
    }

    #[test]
    fn test_all_none_pricing() {
        let tokens = TokenCounts::new(1000, 500, 100, 50);
        let pricing = ModelPricing {
            input_cost_per_token: None,
            output_cost_per_token: None,
            cache_creation_input_token_cost: None,
            cache_read_input_token_cost: None,
        };

        let cost = CostCalculator::calculate_from_pricing(&tokens, &pricing);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_negative_result_protection() {
        // Even though our types use u64 which can't be negative,
        // test that cost calculation doesn't produce negative results
        let tokens = TokenCounts::new(1, 1, 1, 1);
        let pricing = ModelPricing {
            input_cost_per_token: Some(0.0),
            output_cost_per_token: Some(0.0),
            cache_creation_input_token_cost: Some(0.0),
            cache_read_input_token_cost: Some(0.0),
        };

        let cost = CostCalculator::calculate_from_pricing(&tokens, &pricing);
        assert!(cost >= 0.0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_precision_edge_cases() {
        // Test with very small pricing values
        let tokens = TokenCounts::new(1, 1, 1, 1);
        let pricing = ModelPricing {
            input_cost_per_token: Some(0.000000001),             // 1e-9
            output_cost_per_token: Some(0.000000002),            // 2e-9
            cache_creation_input_token_cost: Some(0.0000000015), // 1.5e-9
            cache_read_input_token_cost: Some(0.0000000001),     // 1e-10
        };

        let cost = CostCalculator::calculate_from_pricing(&tokens, &pricing);
        // Should handle very small numbers without precision issues
        assert!(cost > 0.0);
        assert!(cost < 0.00001);
    }

    #[tokio::test]
    async fn test_auto_mode_unknown_model_returns_zero() {
        let fetcher = Arc::new(PricingFetcher::new(true).await);
        let calculator = CostCalculator::new(fetcher);
        let tokens = TokenCounts::new(1000, 500, 0, 0);
        let model = ModelName::new("totally-unknown-model-xyz");

        let result = calculator
            .calculate_with_mode(&tokens, &model, None, CostMode::Auto)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0.0);
    }

    #[tokio::test]
    async fn test_calculate_mode_unknown_model_errors() {
        let fetcher = Arc::new(PricingFetcher::new(true).await);
        let calculator = CostCalculator::new(fetcher);
        let tokens = TokenCounts::new(1000, 500, 0, 0);
        let model = ModelName::new("totally-unknown-model-xyz");

        let result = calculator
            .calculate_with_mode(&tokens, &model, None, CostMode::Calculate)
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CcstatError::UnknownModel(_)));
    }

    #[tokio::test]
    async fn test_auto_mode_unknown_model_with_precalculated() {
        let fetcher = Arc::new(PricingFetcher::new(true).await);
        let calculator = CostCalculator::new(fetcher);
        let tokens = TokenCounts::new(1000, 500, 0, 0);
        let model = ModelName::new("totally-unknown-model-xyz");

        let result = calculator
            .calculate_with_mode(&tokens, &model, Some(1.23), CostMode::Auto)
            .await;

        assert!(result.is_ok());
        assert!((result.unwrap() - 1.23).abs() < f64::EPSILON);
    }
}
