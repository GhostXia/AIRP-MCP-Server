//! AIRP MCP Server — AI Roleplay Data Manager
//!
//! A purely advisory MCP server. All Tools, Resources, and Prompts
//! are suggestions for the MCP Client (Agent). The Agent has full
//! discretion over which capabilities to use and how to use them.
//!
//! AIRP does not call AI APIs, does not perform reasoning, and does
//! not enforce any workflow. It is a toolbox, not an instruction manual.

pub mod error;
pub mod mcp;
pub mod models;
pub mod storage;
pub mod transport;
