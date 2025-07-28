//! MCP (Model Context Protocol) server implementation
//!
//! This module provides a JSON-RPC server that exposes ccusage functionality
//! through the Model Context Protocol. It supports both stdio and HTTP transports.
//!
//! # Available Methods
//!
//! - `daily` - Get daily usage summary with optional filters
//! - `session` - Get session-based usage data
//! - `monthly` - Get monthly usage rollups
//! - `server_info` - Get server information and available methods
//!
//! # Transports
//!
//! ## Stdio Transport
//!
//! The stdio transport reads JSON-RPC requests from stdin and writes responses
//! to stdout, making it suitable for integration with other tools via pipes.
//!
//! ```bash
//! ccusage mcp --transport stdio
//! ```
//!
//! ## HTTP Transport
//!
//! The HTTP transport runs a web server that accepts JSON-RPC requests via POST
//! to the root endpoint.
//!
//! ```bash
//! ccusage mcp --transport http --port 8080
//! ```
//!
//! # Example Request
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "method": "daily",
//!   "params": {
//!     "mode": "auto",
//!     "since": "2024-01-01",
//!     "until": "2024-01-31",
//!     "project": "my-project"
//!   },
//!   "id": 1
//! }
//! ```

use crate::aggregation::{Aggregator, Totals};
use crate::cli::{parse_date_filter, parse_month_filter};
use crate::cost_calculator::CostCalculator;
use crate::data_loader::DataLoader;
use crate::error::{CcusageError, Result};
use crate::filters::{MonthFilter, UsageFilter};
use crate::pricing_fetcher::PricingFetcher;
use crate::types::CostMode;
use jsonrpc_core::{IoHandler, Params, Value};
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use warp::Filter;

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
    since: Option<String>,
    until: Option<String>,
    project: Option<String>,
}

/// Session usage request parameters
#[derive(Debug, Deserialize)]
struct SessionArgs {
    #[serde(default)]
    mode: CostMode,
    since: Option<String>,
    until: Option<String>,
}

/// Monthly usage request parameters
#[derive(Debug, Deserialize)]
struct MonthlyArgs {
    #[serde(default)]
    mode: CostMode,
    since: Option<String>,
    until: Option<String>,
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

        // Build filter
        let mut filter = UsageFilter::new();
        
        if let Some(since_str) = &args.since {
            let since_date = parse_date_filter(since_str)
                .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;
            filter = filter.with_since(since_date);
        }
        
        if let Some(until_str) = &args.until {
            let until_date = parse_date_filter(until_str)
                .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;
            filter = filter.with_until(until_date);
        }
        
        if let Some(project_name) = &args.project {
            filter = filter.with_project(project_name.clone());
        }

        // Apply filters
        let filtered_entries = filter.filter_stream(entries).await;

        // Aggregate
        let daily_data = aggregator
            .aggregate_daily(filtered_entries, args.mode)
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

        // Build filter
        let mut filter = UsageFilter::new();
        
        if let Some(since_str) = &args.since {
            let since_date = parse_date_filter(since_str)
                .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;
            filter = filter.with_since(since_date);
        }
        
        if let Some(until_str) = &args.until {
            let until_date = parse_date_filter(until_str)
                .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;
            filter = filter.with_until(until_date);
        }

        // Apply filters
        let filtered_entries = filter.filter_stream(entries).await;

        // Aggregate
        let session_data = aggregator
            .aggregate_sessions(filtered_entries, args.mode)
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
        let mut monthly_data = Aggregator::aggregate_monthly(&daily_data);
        
        // Apply month filter if provided
        let mut month_filter = MonthFilter::new();
        
        if let Some(since_str) = &args.since {
            let (year, month) = parse_month_filter(since_str)
                .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;
            month_filter = month_filter.with_since(year, month);
        }
        
        if let Some(until_str) = &args.until {
            let (year, month) = parse_month_filter(until_str)
                .map_err(|e| jsonrpc_core::Error::invalid_params(e.to_string()))?;
            month_filter = month_filter.with_until(year, month);
        }
        
        // Filter monthly data
        monthly_data.retain(|monthly| {
            if let Some((year, month)) = monthly
                .month
                .split_once('-')
                .and_then(|(y, m)| Some((y.parse::<i32>().ok()?, m.parse::<u32>().ok()?)))
            {
                if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, 1) {
                    return month_filter.matches_date(&date);
                }
            }
            false
        });

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
        let handler = self.create_handler();

        // Set up stdio streams
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        info!("MCP server listening on stdio...");

        // Process incoming requests
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            debug!("Received request: {}", line);

            // Parse and handle the JSON-RPC request
            match serde_json::from_str::<jsonrpc_core::Request>(&line) {
                Ok(request) => {
                    // Handle the request
                    let response = handler.handle_rpc_request(request).await;
                    
                    // Serialize and write response
                    if let Some(response) = response {
                        let response_str = serde_json::to_string(&response)
                            .map_err(|e| CcusageError::McpServer(format!("Failed to serialize response: {}", e)))?;
                        
                        stdout.write_all(response_str.as_bytes()).await?;
                        stdout.write_all(b"\n").await?;
                        stdout.flush().await?;
                        
                        debug!("Sent response: {}", response_str);
                    }
                }
                Err(e) => {
                    error!("Failed to parse request: {}", e);
                    
                    // Send error response
                    let error_response = jsonrpc_core::Response::from(
                        jsonrpc_core::Error::parse_error(),
                        Some(jsonrpc_core::Version::V2),
                    );
                    
                    let response_str = serde_json::to_string(&error_response)
                        .map_err(|e| CcusageError::McpServer(format!("Failed to serialize error: {}", e)))?;
                    
                    stdout.write_all(response_str.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
            }
        }

        info!("MCP stdio server shutting down");
        Ok(())
    }

    /// Run the MCP server on HTTP
    pub async fn run_http(self, port: u16) -> Result<()> {
        info!("Starting MCP server on HTTP port {}", port);

        // Create handler
        let handler = Arc::new(self.create_handler());

        // Define CORS configuration
        let cors = warp::cors()
            .allow_any_origin()
            .allow_headers(vec!["content-type"])
            .allow_methods(vec!["POST", "GET", "OPTIONS"]);

        // Health check endpoint
        let health = warp::path("health")
            .map(|| warp::reply::json(&serde_json::json!({"status": "ok"})));

        // JSON-RPC endpoint
        let jsonrpc = warp::path::end()
            .and(warp::post())
            .and(warp::body::json())
            .and(warp::any().map(move || handler.clone()))
            .and_then(handle_jsonrpc_request);

        // Combine routes
        let routes = health
            .or(jsonrpc)
            .with(cors)
            .with(warp::trace::request());

        // Start server
        let addr = ([127, 0, 0, 1], port);
        info!("MCP HTTP server listening on http://{}:{}", addr.0.iter().map(|b| b.to_string()).collect::<Vec<_>>().join("."), addr.1);
        
        warp::serve(routes)
            .run(addr)
            .await;

        Ok(())
    }
}

/// Handle JSON-RPC request for HTTP transport
async fn handle_jsonrpc_request(
    request: jsonrpc_core::Request,
    handler: Arc<IoHandler>,
) -> std::result::Result<impl warp::Reply, warp::Rejection> {
    debug!("Received HTTP JSON-RPC request: {:?}", request);
    
    // Handle the request
    let response = handler.handle_rpc_request(request).await;
    
    // Convert response to warp reply
    match response {
        Some(response) => Ok(warp::reply::json(&response)),
        None => {
            // No response needed (notification)
            Ok(warp::reply::json(&serde_json::json!({})))
        }
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
        assert_eq!(args.since, Some("2024-01-01".to_string()));
        assert_eq!(args.until, Some("2024-01-31".to_string()));
    }

    #[test]
    fn test_default_cost_mode() {
        let json = serde_json::json!({});
        let args: DailyArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.mode, CostMode::Auto);
    }
}
