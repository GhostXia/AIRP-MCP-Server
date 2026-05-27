//! HTTP transport for MCP — Streamable HTTP only.
//!
//! AIRP is a pure MCP server. This module only exposes MCP protocol endpoints.
//! No standalone REST API; no AI LLM API calls. All RP logic is driven by the
//! MCP Client (Agent) via Tools / Resources / Prompts.

use axum::{
    routing::{get, post},
    Router,
    response::sse::{Event, Sse},
    extract::State,
    body::Bytes,
    response::IntoResponse,
    http::StatusCode,
};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tracing::info;

use crate::error::Result;
use crate::mcp::AirpMcpServer;

#[derive(Clone)]
pub struct HttpState {
    pub mcp_server: AirpMcpServer,
    pub sse_tx: broadcast::Sender<String>,
}

pub async fn run_http_server(bind: &str, data_dir: &str) -> Result<()> {
    info!("Starting AIRP MCP HTTP server on {}", bind);

    let mcp_server = AirpMcpServer::new(data_dir)?;
    mcp_server.init().await?;

    let (sse_tx, _sse_rx) = broadcast::channel(100);

    let state = HttpState { mcp_server, sse_tx };

    let app = Router::new()
        .route("/mcp/v1", post(handle_mcp_post))
        .route("/mcp/v1", get(handle_mcp_sse))
        .route("/health", get(health_check))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!("AIRP MCP listening on {} (MCP Streamable HTTP only)", bind);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_check() -> &'static str {
    "AIRP MCP Server"
}

async fn handle_mcp_post(
    State(_state): State<HttpState>,
    body: Bytes,
) -> impl IntoResponse {
    let request: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response(),
    };

    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": request.get("id").cloned().unwrap_or(serde_json::Value::Null),
        "result": {},
    });

    response.to_string().into_response()
}

async fn handle_mcp_sse(
    State(state): State<HttpState>,
) -> Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.sse_tx.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx);

    Sse::new(stream.map(|msg| match msg {
        Ok(data) => Ok(Event::default().data(data)),
        Err(_) => Ok(Event::default().data("")),
    }))
    .keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive"),
    )
}
