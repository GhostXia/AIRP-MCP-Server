//! Error types for AIRP MCP Server

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AirpError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Character not found: {0}")]
    CharacterNotFound(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Preset not found: {0}")]
    PresetNotFound(String),

    #[error("Invalid ID: {0}")]
    InvalidId(String),

    #[error("PNG parse error: {0}")]
    PngParse(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

impl From<AirpError> for rmcp::ErrorData {
    fn from(err: AirpError) -> Self {
        rmcp::ErrorData {
            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
            message: err.to_string().into(),
            data: None,
        }
    }
}

pub type Result<T> = std::result::Result<T, AirpError>;
