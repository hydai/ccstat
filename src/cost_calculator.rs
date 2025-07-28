//! Cost calculator module for computing usage costs

use crate::error::{CcusageError, Result};
use crate::pricing_fetcher::PricingFetcher;
use crate::types::{CostMode, ModelName, ModelPricing, TokenCounts};
use std::sync::Arc;
use tracing::debug;

/// Calculates costs based on token usage and pricing
pub struct CostCalculator {
    /// Pricing fetcher instance
    pricing_fetcher: Arc<PricingFetcher>,
}

impl CostCalculator {
    /// Create a new CostCalculator
    pub fn new(pricing_fetcher: Arc<PricingFetcher>) -> Self {
        Self { pricing_fetcher }
    }

    /// Calculate cost for token usage
    pub async fn calculate_cost(
        &self,
        tokens: &TokenCounts,
        model_name: &ModelName,
    ) -> Result<f64> {
        let pricing = self
            .pricing_fetcher
            .get_model_pricing(model_name.as_str())
            .await?
            .ok_or_else(|| CcusageError::UnknownModel(model_name.clone()))?;

        Ok(Self::calculate_from_pricing(tokens, &pricing))
    }

    /// Calculate cost from pricing data
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
                    self.calculate_cost(tokens, model_name).await
                }
            }
            CostMode::Calculate => self.calculate_cost(tokens, model_name).await,
            CostMode::Display => pre_calculated.ok_or_else(|| {
                CcusageError::InvalidArgument(
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
}
