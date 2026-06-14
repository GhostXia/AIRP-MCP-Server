//! HTTP transport for MCP — Streamable HTTP (MCP spec 2025-06-18).
//!
//! Mounts rmcp's `StreamableHttpService` at `/mcp/v1`. rmcp handles the real
//! protocol: JSON-RPC dispatch to the server (R1), `initialize` →
//! `notifications/initialized` → `tools/list`/`tools/call`/`resources/read`
//! lifecycle (R2), `Mcp-Session-Id` session header + per-session SSE (R3),
//! `MCP-Protocol-Version` validation (R4), Accept-based content negotiation
//! (R5), and JSON-RPC error codes (R8). This module adds the edge concerns:
//! optional bearer auth (R6) and CORS (R7).

use std::sync::Arc;

use axum::{
    Router as AxumRouter,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

use crate::error::Result;
use crate::mcp::AirpMcpServer;

/// Optional bearer token, baked into the auth middleware as state.
#[derive(Clone)]
struct AuthState {
    token: Option<Arc<String>>,
}

pub async fn run_http_server(bind: &str, data_dir: &str) -> Result<()> {
    info!("Starting AIRP MCP HTTP server on {}", bind);

    let server = AirpMcpServer::new(data_dir)?;
    server.init().await?;

    let auth_token = std::env::var("AIRP_HTTP_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    let is_loopback =
        bind.starts_with("127.") || bind.starts_with("localhost") || bind.starts_with("[::1]");
    match (&auth_token, is_loopback) {
        (Some(_), _) => info!("HTTP bearer auth enabled (AIRP_HTTP_TOKEN set)"),
        (None, false) => warn!(
            "HTTP bound to {} with NO auth (AIRP_HTTP_TOKEN unset). Any device on the \
             network can call read/write tools. Treat the LAN as trusted; set \
             AIRP_HTTP_TOKEN, and never expose this to the public internet.",
            bind
        ),
        (None, true) => {}
    }

    let app = build_router(server, auth_token);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(
        "AIRP MCP listening on {} (Streamable HTTP at /mcp/v1)",
        bind
    );

    axum::serve(listener, app).await?;
    Ok(())
}

/// Assemble the axum app: rmcp Streamable HTTP at `/mcp/v1` (+ bearer auth +
/// CORS) and an open `/health`. Extracted so tests can drive it in-process via
/// `tower::ServiceExt::oneshot` without binding a socket.
pub(crate) fn build_router(server: AirpMcpServer, auth_token: Option<String>) -> AxumRouter {
    // Serve the handler directly (rmcp blanket-impls Service for ServerHandler,
    // dispatching tools/list etc. to our hand-written methods). Do NOT wrap it
    // in rmcp's router::Router: that router answers tools/list from its own
    // route table — empty here, since our tools aren't macro-registered — and
    // never delegates, so every tool would vanish over this transport. All
    // sessions share the same Storage (AirpMcpServer is Clone over Arc).
    let mcp_service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        // Allow non-loopback Host headers: LAN deployment (PC backend + phone on
        // the same wifi) is a supported use. Bearer token + "trust your LAN" is
        // the security model; rmcp's default loopback-only Host check would
        // otherwise reject LAN clients (DNS-rebinding guard).
        StreamableHttpServerConfig::default().disable_allowed_hosts(),
    );

    let auth = AuthState {
        token: auth_token.map(Arc::new),
    };

    // R7: allow the MCP headers and expose the session id to browsers.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([
            header::HeaderName::from_static("mcp-session-id"),
            header::HeaderName::from_static("mcp-protocol-version"),
        ]);

    AxumRouter::new()
        .route_service("/mcp/v1", mcp_service)
        // Auth applies only to routes defined above this layer; /health stays open.
        .route_layer(middleware::from_fn_with_state(auth, require_bearer_auth))
        .route("/health", get(health_check))
        .layer(cors)
}

async fn health_check() -> &'static str {
    "AIRP MCP Server"
}

/// R6: reject `/mcp/v1` requests lacking a valid bearer token, when one is
/// configured. No-op when no token is set (loopback/LAN-trust default).
async fn require_bearer_auth(State(auth): State<AuthState>, req: Request, next: Next) -> Response {
    if let Some(token) = auth.token.as_deref() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt; // oneshot

    async fn test_app(token: Option<&str>) -> (AxumRouter, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let server = AirpMcpServer::new(&path).unwrap();
        server.init().await.unwrap();
        (build_router(server, token.map(|t| t.to_string())), dir)
    }

    /// A spec-shaped `initialize` POST. Host header is required by rmcp's DNS-
    /// rebinding guard even with allowed_hosts disabled; Accept must list both
    /// json and event-stream.
    fn initialize_request(token: Option<&str>) -> Request<Body> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "airp-test", "version": "0" }
            }
        })
        .to_string();
        let mut b = Request::builder()
            .method("POST")
            .uri("/mcp/v1")
            .header(header::HOST, "localhost")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(t) = token {
            b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        b.body(Body::from(body)).unwrap()
    }

    #[tokio::test]
    async fn health_is_open_even_with_token() {
        let (app, _dir) = test_app(Some("secret")).await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_rejects_missing_bearer_when_token_set() {
        let (app, _dir) = test_app(Some("secret")).await;
        let resp = app.oneshot(initialize_request(None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn initialize_returns_session_id() {
        let (app, _dir) = test_app(None).await; // no auth configured
        let resp = app.oneshot(initialize_request(None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers().get("mcp-session-id").is_some(),
            "initialize response must carry Mcp-Session-Id (R3)"
        );
    }

    #[tokio::test]
    async fn initialize_accepts_valid_bearer() {
        let (app, _dir) = test_app(Some("secret")).await;
        let resp = app
            .oneshot(initialize_request(Some("secret")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("mcp-session-id").is_some());
    }

    /// Pull the JSON-RPC payload out of a `/mcp/v1` response. rmcp answers POSTs
    /// with `text/event-stream`, so the JSON rides in an SSE `data:` frame; fall
    /// back to the whole body for plain `application/json`.
    async fn read_rpc_json(resp: Response) -> serde_json::Value {
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        // rmcp opens the SSE stream with an empty keep-alive frame ("data:\n
        // id: 0\nretry: 3000") before the real message, so take the first
        // non-empty `data:` payload, not just the first one.
        let payload = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:").map(str::trim))
            .find(|p| !p.is_empty())
            .unwrap_or_else(|| text.trim());
        serde_json::from_str(payload)
            .unwrap_or_else(|e| panic!("response body is not JSON-RPC ({e}): {text}"))
    }

    /// A post-initialize request carrying the negotiated session id and protocol
    /// version (both required by rmcp once the session exists).
    fn mcp_post(method: &str, id: Option<u32>, session: &str, protocol: &str) -> Request<Body> {
        let mut msg = serde_json::json!({ "jsonrpc": "2.0", "method": method });
        if let Some(id) = id {
            msg["id"] = id.into();
        }
        Request::builder()
            .method("POST")
            .uri("/mcp/v1")
            .header(header::HOST, "localhost")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header(header::CONTENT_TYPE, "application/json")
            .header("mcp-session-id", session)
            .header("mcp-protocol-version", protocol)
            .body(Body::from(msg.to_string()))
            .unwrap()
    }

    /// The real handshake, decoded end to end: initialize result body (R1/R5),
    /// session lifecycle (R2/R3), and the full tool registry served over HTTP.
    /// Unlike the status+header checks above, this asserts the actual JSON-RPC
    /// payloads, so it fails if rmcp returns 200 with an error frame.
    #[tokio::test]
    async fn full_handshake_lists_all_38_tools() {
        let (app, _dir) = test_app(None).await;

        // 1. initialize -> decode the result, not just the status line.
        let resp = app.clone().oneshot(initialize_request(None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let session = resp
            .headers()
            .get("mcp-session-id")
            .expect("initialize must mint a session id (R3)")
            .to_str()
            .unwrap()
            .to_string();
        let init = read_rpc_json(resp).await;
        assert!(init.get("error").is_none(), "initialize errored: {init}");
        assert_eq!(
            init["result"]["serverInfo"]["name"], "airp-mcp-server",
            "initialize result must carry our serverInfo (R1)"
        );
        // Echo back whatever version the server negotiated, rather than guessing.
        let protocol = init["result"]["protocolVersion"]
            .as_str()
            .expect("result must carry a protocolVersion (R5)")
            .to_string();

        // 2. notifications/initialized completes the lifecycle (R2).
        let resp = app
            .clone()
            .oneshot(mcp_post(
                "notifications/initialized",
                None,
                &session,
                &protocol,
            ))
            .await
            .unwrap();
        assert!(
            resp.status().is_success(),
            "initialized notification rejected: {}",
            resp.status()
        );

        // 3. tools/list over the live session -> assert the whole registry (R2).
        let resp = app
            .clone()
            .oneshot(mcp_post("tools/list", Some(2), &session, &protocol))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let listed = read_rpc_json(resp).await;
        assert!(
            listed.get("error").is_none(),
            "tools/list errored: {listed}"
        );
        let tools = listed["result"]["tools"]
            .as_array()
            .expect("tools/list must return a tools array");
        assert_eq!(tools.len(), 38, "expected all 38 tools over HTTP");
        assert!(
            tools.iter().any(|t| t["name"] == "list_characters"),
            "registry must include list_characters"
        );
    }
}
