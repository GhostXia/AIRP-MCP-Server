//! AIRP MCP Server
//! 
//! 一个为角色扮演(RP)数据管理而设计的MCP Server。
//! 支持stdio和HTTP两种传输模式。

use clap::{Parser, Subcommand};
use tracing::info;

use airp_mcp_server::error::AirpError;
use airp_mcp_server::mcp;
use airp_mcp_server::transport;

#[derive(Parser)]
#[command(name = "airp-mcp")]
#[command(about = "AIRP MCP Server - Roleplay data management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run MCP server with stdio transport (for Claude Code, Cursor, etc.)
    Mcp {
        /// Data root directory
        #[arg(short, long, default_value = "./data")]
        data_dir: String,
    },
    /// Run HTTP server with MCP endpoint
    Serve {
        /// Bind address
        #[arg(short, long, default_value = "127.0.0.1:3000")]
        bind: String,
        /// Data root directory
        #[arg(short, long, default_value = "./data")]
        data_dir: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), AirpError> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Mcp { data_dir } => {
            info!("Starting AIRP MCP Server (stdio mode)");
            info!("Data directory: {}", data_dir);

            let server = mcp::AirpMcpServer::new(&data_dir)?;
            transport::stdio::run_stdio_server(server).await.map_err(|e| AirpError::Transport(e.to_string()))?;
        }
        Commands::Serve { bind, data_dir } => {
            info!("Starting AIRP HTTP Server");
            info!("Bind: {}, Data directory: {}", bind, data_dir);

            transport::http::run_http_server(&bind, &data_dir).await?;
        }
    }

    Ok(())
}
