//! HTTP transport for MCP — Streamable HTTP only.
//!
//! AIRP is a pure MCP server. This module only exposes MCP protocol endpoints.
//! No standalone REST API; no AI LLM API calls. All RP logic is driven by the
//! MCP Client (Agent) via Tools / Resources / Prompts.

use axum::{
    Router,
    body::Bytes,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::sse::{Event, Sse},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::error::Result;
use crate::mcp::AirpMcpServer;

#[derive(Clone)]
pub struct HttpState {
    pub mcp_server: AirpMcpServer,
    pub sse_tx: broadcast::Sender<String>,
    /// Optional bearer token (from env AIRP_HTTP_TOKEN). When set, every MCP
    /// request must carry `Authorization: Bearer <token>`. Opt-in — unset = no
    /// auth (backward compatible), intended for a trusted loopback/LAN.
    pub auth_token: Option<String>,
}

pub async fn run_http_server(bind: &str, data_dir: &str) -> Result<()> {
    info!("Starting AIRP MCP HTTP server on {}", bind);

    let mcp_server = AirpMcpServer::new(data_dir)?;
    mcp_server.init().await?;

    let (sse_tx, _sse_rx) = broadcast::channel(100);

    let auth_token = std::env::var("AIRP_HTTP_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    let is_loopback =
        bind.starts_with("127.") || bind.starts_with("localhost") || bind.starts_with("[::1]");
    match (&auth_token, is_loopback) {
        (Some(_), _) => info!("HTTP bearer auth enabled (AIRP_HTTP_TOKEN set)"),
        (None, false) => warn!(
            "HTTP bound to {} with NO auth (AIRP_HTTP_TOKEN unset). Any device on \
             the network can call read/write tools. Treat the LAN as trusted; set \
             AIRP_HTTP_TOKEN, and never expose this to the public internet.",
            bind
        ),
        (None, true) => {}
    }

    let state = HttpState {
        mcp_server,
        sse_tx,
        auth_token,
    };

    let app = Router::new()
        .route("/mcp/v1", post(handle_mcp_post))
        .route("/mcp/v1", get(handle_mcp_sse))
        // Auth applies only to routes defined above this layer; /health stays open.
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer_auth,
        ))
        .route("/health", get(health_check))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!("AIRP MCP listening on {} (MCP Streamable HTTP only)", bind);

    axum::serve(listener, app).await?;
    Ok(())
}

/// Reject MCP requests lacking a valid bearer token, when one is configured.
/// No-op when auth_token is None (loopback/LAN-trust default).
async fn require_bearer_auth(State(state): State<HttpState>, req: Request, next: Next) -> Response {
    if let Some(token) = state.auth_token.as_deref() {
        let presented = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let ok = match presented {
            Some(t) => constant_time_eq(t.as_bytes(), token.as_bytes()),
            None => false,
        };
        if !ok {
            return (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response();
        }
    }
    next.run(req).await
}

/// Length-checked constant-time byte compare (avoids token-timing leaks).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn health_check() -> &'static str {
    "AIRP MCP Server"
}

async fn handle_mcp_post(State(_state): State<HttpState>, body: Bytes) -> impl IntoResponse {
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
