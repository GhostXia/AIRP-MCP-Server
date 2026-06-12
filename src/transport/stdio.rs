//! Stdio transport for MCP (for Claude Code, Cursor, etc.)

use crate::mcp::AirpMcpServer;
use rmcp::serve_server;

pub async fn run_stdio_server(
    server: AirpMcpServer,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = rmcp::handler::server::router::Router::new(server);
    let running = serve_server(service, rmcp::transport::io::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
