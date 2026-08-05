//! blender-mcp-rs binary entry point.
//!
//! Mirrors the Python `main()`: when launched by hand (stdin is a TTY) the
//! server looks like it is "hanging" while it silently waits for an MCP client,
//! so a hint is printed to stderr. Logging goes to stderr, never to the stdio
//! protocol on stdout.

use std::io::IsTerminal;

use rmcp::{ServiceExt, transport::stdio};

use blender_mcp_rs::server::BlenderServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let interactive = std::io::stdin().is_terminal();
    if interactive {
        eprintln!(
            "BlenderMCP is an MCP server and is meant to be launched by your MCP \
             client (Claude Desktop, Cursor, VS Code, ...), not run by hand. \
             It will now wait silently for a client on stdin -- that is normal, \
             not a hang. Press Ctrl-C to exit. \
             Setup guide: https://github.com/ahujasid/blender-mcp#installation"
        );
    }

    let service = BlenderServer::default();
    let running = service.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}