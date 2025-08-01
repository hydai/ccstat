//! Property-based tests for ccstat using proptest

use ccstat::{
    cost_calculator::CostCalculator,
    types::{ISOTimestamp, ModelName, ModelPricing, SessionId, TokenCounts, UsageEntry},
};
use chrono::{Datelike, TimeZone, Utc};
use proptest::prelude::*;
use std::sync::Arc;

// Strategies for generating test data

prop_compose! {
    fn arb_token_counts()(
        input in 0u64..10_000_000,
        output in 0u64..5_000_000,
        cache_creation in 0u64..1_000_000,
        cache_read in 0u64..500_000,
    ) -> TokenCounts {
        TokenCounts::new(input, output, cache_creation, cache_read)
    }
}

prop_compose! {
    fn arb_model_pricing()(
        input_cost in prop::option::of(0.000001f64..0.01),
        output_cost in prop::option::of(0.000001f64..0.02),
        cache_creation_cost in prop::option::of(0.000001f64..0.015),
        cache_read_cost in prop::option::of(0.0000001f64..0.001),
    ) -> ModelPricing {
        ModelPricing {
            input_cost_per_token: input_cost,
            output_cost_per_token: output_cost,
            cache_creation_input_token_cost: cache_creation_cost,
            cache_read_input_token_cost: cache_read_cost,
        }
    }
}

prop_compose! {
    fn arb_timestamp()(
        secs in 1577836800i64..1735689600i64, // 2020-01-01 to 2025-01-01
        nanos in 0u32..1_000_000_000u32,
    ) -> ISOTimestamp {
        let dt = Utc.timestamp_opt(secs, nanos).unwrap();
        ISOTimestamp::new(dt)
    }
}

prop_compose! {
    fn arb_model_name()(
        name in prop::sample::select(vec![
            "claude-3-opus",
            "claude-3-sonnet",
            "claude-3-haiku",
            "claude-3.5-sonnet",
            "claude-3-opus-20240229",
            "claude-3-sonnet-20240229",
        ])
    ) -> ModelName {
        ModelName::new(name)
    }
}

prop_compose! {
    fn arb_session_id()(
        id in "[a-zA-Z0-9]{8}-[a-zA-Z0-9]{4}-[a-zA-Z0-9]{4}-[a-zA-Z0-9]{4}-[a-zA-Z0-9]{12}"
    ) -> SessionId {
        SessionId::new(id)
    }
}

prop_compose! {
    fn arb_usage_entry()(
        session_id in arb_session_id(),
        timestamp in arb_timestamp(),
        model in arb_model_name(),
        tokens in arb_token_counts(),
        total_cost in prop::option::of(0.0f64..100.0),
        project in prop::option::of("[a-z]{5,10}"),
        instance_id in prop::option::of("[a-z0-9]{5,15}"),
    ) -> UsageEntry {
        UsageEntry {
            session_id,
            timestamp,
            model,
            tokens,
            total_cost,
            project,
            instance_id,
        }
    }
}

proptest! {
    #[test]
    fn test_cost_calculation_never_negative(
        tokens in arb_token_counts(),
        pricing in arb_model_pricing(),
    ) {
        let cost = CostCalculator::calculate_from_pricing(&tokens, &pricing);
        prop_assert!(cost >= 0.0);
    }

    #[test]
    fn test_cost_calculation_consistency(
        tokens in arb_token_counts(),
        pricing in arb_model_pricing(),
    ) {
        let cost1 = CostCalculator::calculate_from_pricing(&tokens, &pricing);
        let cost2 = CostCalculator::calculate_from_pricing(&tokens, &pricing);
        prop_assert_eq!(cost1, cost2);
    }

    #[test]
    fn test_cost_monotonic_increase(
        base_tokens in arb_token_counts(),
        additional_input in 0u64..1_000_000,
        pricing in arb_model_pricing(),
    ) {
        let cost_base = CostCalculator::calculate_from_pricing(&base_tokens, &pricing);

        let increased_tokens = TokenCounts::new(
            base_tokens.input_tokens + additional_input,
            base_tokens.output_tokens,
            base_tokens.cache_creation_tokens,
            base_tokens.cache_read_tokens,
        );
        let cost_increased = CostCalculator::calculate_from_pricing(&increased_tokens, &pricing);

        // Cost should increase or stay the same when tokens increase
        prop_assert!(cost_increased >= cost_base);
    }

    #[test]
    fn test_token_addition_associative(
        t1 in arb_token_counts(),
        t2 in arb_token_counts(),
        t3 in arb_token_counts(),
    ) {
        // Ensure no overflow by limiting values
        let safe_t1 = TokenCounts::new(
            t1.input_tokens / 4,
            t1.output_tokens / 4,
            t1.cache_creation_tokens / 4,
            t1.cache_read_tokens / 4,
        );
        let safe_t2 = TokenCounts::new(
            t2.input_tokens / 4,
            t2.output_tokens / 4,
            t2.cache_creation_tokens / 4,
            t2.cache_read_tokens / 4,
        );
        let safe_t3 = TokenCounts::new(
            t3.input_tokens / 4,
            t3.output_tokens / 4,
            t3.cache_creation_tokens / 4,
            t3.cache_read_tokens / 4,
        );

        // (t1 + t2) + t3 == t1 + (t2 + t3)
        let left = (safe_t1 + safe_t2) + safe_t3;
        let right = safe_t1 + (safe_t2 + safe_t3);

        prop_assert_eq!(left, right);
    }

    #[test]
    fn test_token_addition_commutative(
        t1 in arb_token_counts(),
        t2 in arb_token_counts(),
    ) {
        // Ensure no overflow
        let safe_t1 = TokenCounts::new(
            t1.input_tokens / 2,
            t1.output_tokens / 2,
            t1.cache_creation_tokens / 2,
            t1.cache_read_tokens / 2,
        );
        let safe_t2 = TokenCounts::new(
            t2.input_tokens / 2,
            t2.output_tokens / 2,
            t2.cache_creation_tokens / 2,
            t2.cache_read_tokens / 2,
        );

        // t1 + t2 == t2 + t1
        prop_assert_eq!(safe_t1 + safe_t2, safe_t2 + safe_t1);
    }

    #[test]
    fn test_timestamp_ordering_transitive(
        secs1 in 1577836800i64..1735689600i64,
        secs2 in 1577836800i64..1735689600i64,
        secs3 in 1577836800i64..1735689600i64,
    ) {
        let ts1 = ISOTimestamp::new(Utc.timestamp_opt(secs1, 0).unwrap());
        let ts2 = ISOTimestamp::new(Utc.timestamp_opt(secs2, 0).unwrap());
        let ts3 = ISOTimestamp::new(Utc.timestamp_opt(secs3, 0).unwrap());

        // If ts1 <= ts2 and ts2 <= ts3, then ts1 <= ts3
        if ts1 <= ts2 && ts2 <= ts3 {
            prop_assert!(ts1 <= ts3);
        }
    }

    #[test]
    fn test_usage_entry_serialization_roundtrip(
        entry in arb_usage_entry()
    ) {
        // Serialize and deserialize should produce the same entry
        let serialized = serde_json::to_string(&entry).unwrap();
        let deserialized: UsageEntry = serde_json::from_str(&serialized).unwrap();

        prop_assert_eq!(entry.session_id, deserialized.session_id);
        prop_assert_eq!(entry.timestamp, deserialized.timestamp);
        prop_assert_eq!(entry.model, deserialized.model);
        prop_assert_eq!(entry.tokens, deserialized.tokens);

        // For floating point comparison, check within epsilon
        match (entry.total_cost, deserialized.total_cost) {
            (Some(a), Some(b)) => prop_assert!((a - b).abs() < 1e-10),
            (None, None) => {},
            _ => prop_assert!(false, "Cost mismatch: {:?} vs {:?}", entry.total_cost, deserialized.total_cost),
        }

        prop_assert_eq!(entry.project, deserialized.project);
        prop_assert_eq!(entry.instance_id, deserialized.instance_id);
    }

    #[test]
    fn test_date_filter_parsing_valid_formats(
        year in 2020i32..2030,
        month in 1u32..=12,
        day in 1u32..=28, // Using 28 to avoid invalid dates
    ) {
        let date_str = format!("{year:04}-{month:02}-{day:02}");
        let result = ccstat::cli::parse_date_filter(&date_str);
        prop_assert!(result.is_ok());

        let parsed_date = result.unwrap();
        prop_assert_eq!(parsed_date.year(), year);
        prop_assert_eq!(parsed_date.month(), month);
        prop_assert_eq!(parsed_date.day(), day);
    }

    #[test]
    fn test_month_filter_parsing_valid_formats(
        year in 2020i32..2030,
        month in 1u32..=12,
    ) {
        let month_str = format!("{year:04}-{month:02}");
        let result = ccstat::cli::parse_month_filter(&month_str);
        prop_assert!(result.is_ok());

        let (parsed_year, parsed_month) = result.unwrap();
        prop_assert_eq!(parsed_year, year);
        prop_assert_eq!(parsed_month, month);
    }
}

#[cfg(test)]
mod aggregation_property_tests {
    use super::*;
    use ccstat::{
        aggregation::{Aggregator, Totals},
        filters::UsageFilter,
        pricing_fetcher::PricingFetcher,
        types::CostMode,
    };
    use futures::{StreamExt, stream};

    proptest! {
        #[test]
        fn test_aggregation_totals_sum_correctly(
            entries in prop::collection::vec(arb_usage_entry(), 1..50)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
                let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
                let aggregator = Aggregator::new(cost_calculator);

                let entries_stream = stream::iter(entries.clone().into_iter().map(Ok));
                let daily_data = aggregator
                    .aggregate_daily(entries_stream, CostMode::Calculate)
                    .await
                    .unwrap();

                let totals = Totals::from_daily(&daily_data);

                // Total tokens should equal sum of all daily totals
                let expected_input: u64 = daily_data.iter()
                    .map(|d| d.tokens.input_tokens)
                    .sum();
                let expected_output: u64 = daily_data.iter()
                    .map(|d| d.tokens.output_tokens)
                    .sum();

                assert_eq!(totals.tokens.input_tokens, expected_input);
                assert_eq!(totals.tokens.output_tokens, expected_output);
            });
        }

        #[test]
        fn test_filter_consistency(
            entries in prop::collection::vec(arb_usage_entry(), 1..20),
            filter_days in 1i64..365,
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let now = chrono::Utc::now().date_naive();
            let filter_date = now - chrono::Duration::days(filter_days);

            rt.block_on(async {
                let filter = UsageFilter::new()
                    .with_since(filter_date);

                let entries_stream = stream::iter(entries.into_iter().map(Ok));
                let filtered: Vec<_> = filter
                    .filter_stream(entries_stream)
                    .await
                    .collect::<Vec<_>>()
                    .await;

                // All filtered entries should match the filter
                for entry in filtered.iter().flatten() {
                    let entry_date = entry.timestamp.as_ref().date_naive();
                    assert!(entry_date >= filter_date);
                }
            });
        }
    }
}
