//! MCP (Model Context Protocol) server implementation

use crate::aggregation::{Aggregator, Totals};
use crate::cost_calculator::CostCalculator;
use crate::data_loader::DataLoader;
use crate::error::{CcusageError, Result};
use crate::pricing_fetcher::PricingFetcher;
use crate::types::CostMode;
use jsonrpc_core::{IoHandler, Params, Value};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// MCP server for providing API access to usage data
pub struct McpServer {
    /// Data loader instance
    data_loader: Arc<RwLock<DataLoader>>,
    /// Cost calculator instance
    _cost_calculator: Arc<CostCalculator>,
    /// Aggregator instance
    aggregator: Arc<Aggregator>,
}

/// Daily usage request parameters
#[derive(Debug, Deserialize)]
struct DailyArgs {
    #[serde(default)]
    mode: CostMode,
    #[serde(rename = "since")]
    _since: Option<String>,
    #[serde(rename = "until")]
    _until: Option<String>,
}

/// Session usage request parameters
#[derive(Debug, Deserialize)]
struct SessionArgs {
    #[serde(default)]
    mode: CostMode,
    #[serde(rename = "since")]
    _since: Option<String>,
    #[serde(rename = "until")]
    _until: Option<String>,
}

/// Monthly usage request parameters
#[derive(Debug, Deserialize)]
struct MonthlyArgs {
    #[serde(default)]
    mode: CostMode,
    #[serde(rename = "since")]
    _since: Option<String>,
    #[serde(rename = "until")]
    _until: Option<String>,
}

impl McpServer {
    /// Create a new MCP server
    pub async fn new() -> Result<Self> {
        let data_loader = Arc::new(RwLock::new(DataLoader::new().await?));
        let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(Aggregator::new(cost_calculator.clone()));

        Ok(Self {
            data_loader,
            _cost_calculator: cost_calculator,
            aggregator,
        })
    }

    /// Create JSON-RPC handler
    pub fn create_handler(&self) -> IoHandler {
        let mut handler = IoHandler::new();

        // Register methods
        let loader = self.data_loader.clone();
        let aggregator = self.aggregator.clone();
        handler.add_method("daily", move |params: Params| {
            let loader = loader.clone();
            let aggregator = aggregator.clone();
            Box::pin(async move { Self::handle_daily(params, loader, aggregator).await })
        });

        let loader = self.data_loader.clone();
        let aggregator = self.aggregator.clone();
        handler.add_method("session", move |params: Params| {
            let loader = loader.clone();
            let aggregator = aggregator.clone();
            Box::pin(async move { Self::handle_session(params, loader, aggregator).await })
        });

        let loader = self.data_loader.clone();
        let aggregator = self.aggregator.clone();
        handler.add_method("monthly", move |params: Params| {
            let loader = loader.clone();
            let aggregator = aggregator.clone();
            Box::pin(async move { Self::handle_monthly(params, loader, aggregator).await })
        });

        // Add server info method
        handler.add_method("server_info", |_params| async move {
            Ok(serde_json::json!({
                "name": "ccusage MCP Server",
                "version": crate::VERSION,
                "methods": ["daily", "session", "monthly", "server_info"],
            }))
        });

        handler
    }

    /// Handle daily usage request
    async fn handle_daily(
        params: Params,
        loader: Arc<RwLock<DataLoader>>,
        aggregator: Arc<Aggregator>,
    ) -> jsonrpc_core::Result<Value> {
        let args: DailyArgs = params
            .parse()
            .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;

        debug!("Handling daily request with args: {:?}", args);

        // Load data
        let loader = loader.read().await;
        let entries = loader.load_usage_entries();

        // Apply date filters if provided
        // TODO: Implement date filtering on the stream

        // Aggregate
        let daily_data = aggregator
            .aggregate_daily(entries, args.mode)
            .await
            .map_err(|e| jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;

        let totals = Totals::from_daily(&daily_data);

        Ok(serde_json::json!({
            "daily": daily_data,
            "totals": totals,
        }))
    }

    /// Handle session usage request
    async fn handle_session(
        params: Params,
        loader: Arc<RwLock<DataLoader>>,
        aggregator: Arc<Aggregator>,
    ) -> jsonrpc_core::Result<Value> {
        let args: SessionArgs = params
            .parse()
            .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;

        debug!("Handling session request with args: {:?}", args);

        // Load data
        let loader = loader.read().await;
        let entries = loader.load_usage_entries();

        // Aggregate
        let session_data = aggregator
            .aggregate_sessions(entries, args.mode)
            .await
            .map_err(|e| jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;

        let totals = Totals::from_sessions(&session_data);

        Ok(serde_json::json!({
            "sessions": session_data,
            "totals": totals,
        }))
    }

    /// Handle monthly usage request
    async fn handle_monthly(
        params: Params,
        loader: Arc<RwLock<DataLoader>>,
        aggregator: Arc<Aggregator>,
    ) -> jsonrpc_core::Result<Value> {
        let args: MonthlyArgs = params
            .parse()
            .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;

        debug!("Handling monthly request with args: {:?}", args);

        // Load data
        let loader = loader.read().await;
        let entries = loader.load_usage_entries();

        // First aggregate by day
        let daily_data = aggregator
            .aggregate_daily(entries, args.mode)
            .await
            .map_err(|e| jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;

        // Then aggregate to monthly
        let monthly_data = Aggregator::aggregate_monthly(&daily_data);

        let mut totals = Totals::default();
        for monthly in &monthly_data {
            totals.tokens += monthly.tokens;
            totals.total_cost += monthly.total_cost;
        }

        Ok(serde_json::json!({
            "monthly": monthly_data,
            "totals": totals,
        }))
    }

    /// Run the MCP server on stdio
    pub async fn run_stdio(self) -> Result<()> {
        info!("Starting MCP server on stdio");

        // Create handler
        let _handler = self.create_handler();

        // TODO: Implement stdio transport
        // This would involve reading JSON-RPC requests from stdin
        // and writing responses to stdout

        Err(CcusageError::McpServer(
            "Stdio transport not yet implemented".to_string(),
        ))
    }

    /// Run the MCP server on HTTP
    pub async fn run_http(self, port: u16) -> Result<()> {
        info!("Starting MCP server on HTTP port {}", port);

        // Create handler
        let _handler = self.create_handler();

        // TODO: Implement HTTP server
        // This would involve setting up a web server (e.g., using warp or axum)
        // to handle JSON-RPC requests over HTTP

        Err(CcusageError::McpServer(
            "HTTP transport not yet implemented".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daily_args_parsing() {
        let json = serde_json::json!({
            "mode": "Calculate",
            "since": "2024-01-01",
            "until": "2024-01-31"
        });

        let args: DailyArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.mode, CostMode::Calculate);
        assert_eq!(args._since, Some("2024-01-01".to_string()));
        assert_eq!(args._until, Some("2024-01-31".to_string()));
    }

    #[test]
    fn test_default_cost_mode() {
        let json = serde_json::json!({});
        let args: DailyArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.mode, CostMode::Auto);
    }
}
