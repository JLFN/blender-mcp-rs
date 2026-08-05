//! blender-mcp-rs is a Rust port of the ["BlenderMCP"](https://github.com/ahujasid/blender-mcp)
//! standalone MCP server.
//!
//! The Blender addon (see `addon/addon.py`) runs inside Blender and exposes a
//! TCP JSON socket server. This crate is the MCP server that sits between that
//! socket and MCP clients (Claude, Cursor, VS Code, ...): it exposes each
//! Blender operation as an MCP tool, forwards the arguments to the addon over
//! the socket, and returns the formatted result.
//!
//! The port was performed 1:1 against the Python `src/blender_mcp/server.py`,
//! with one intentional deviation: telemetry has been removed entirely (no
//! collection, no consent checks, no uploads).
#![warn(missing_docs)]

pub mod bbox;
pub mod connection;
pub mod server;
pub mod tools;
pub mod util;