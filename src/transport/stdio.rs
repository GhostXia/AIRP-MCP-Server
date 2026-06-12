//! Stdio transport for MCP (for Claude Code, Cursor, etc.)

use crate::mcp::AirpMcpServer;
use rmcp::serve_server;

pub async fn run_stdio_server(
    server: AirpMcpServer,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Serve the handler directly — rmcp blanket-impls Service for ServerHandler,
    // so tools/list etc. reach our hand-written methods. Wrapping in rmcp's
    // router::Router would shadow them with its own (empty) route table and the
    // client would see zero tools.
    let running = serve_server(server, rmcp::transport::io::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
