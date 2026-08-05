# blender-mcp-rs

A 1:1 Rust port of the [BlenderMCP](https://github.com/ahujasid/blender-mcp)
standalone MCP server (the `src/blender_mcp/server.py` component), written
against the [rmcp](https://crates.io/crates/rmcp) Rust MCP SDK.

BlenderMCP lets Claude (or any MCP client) control Blender: inspecting scenes,
querying objects, executing Python against the running Blender instance,
capturing viewport screenshots, and importing assets from PolyHaven,
Sketchfab, Hyper3D Rodin, and Hunyuan3D.

## Architecture

The Blender addon (`addon/addon.py`, Python, unmodified from the original)
runs inside Blender and exposes a TCP JSON socket server on `localhost:9876`.
It cannot be ported to Rust because it drives `bpy` from inside Blender's
Python interpreter. This crate is the MCP server that sits between that socket
and MCP clients:

1. An MCP client calls a tool (for example `get_object_info`).
2. `src/server.rs` forwards the arguments to the addon over the socket as a
   `{"type": ..., "params": {...}}` command.
3. The addon executes the command against Blender and replies with a
   `{"status": ..., "result": ...}` JSON object.
4. The tool formats the result exactly as the Python original did and returns
   it to the client.

Responses are matched to commands purely by stream ordering; a single mutex
is held across send and receive so concurrent tool calls can never cross their
responses. A dead socket is detected on the next real command and reconnected
then, mirroring the Python behavior.

## Telemetry

Telemetry has been removed entirely. The original server's telemetry
collection, consent checks, and screenshot uploads are not ported, and no
metrics or usage data leave this process.

## Building

Requires Rust (edition 2021). No Blender installation is needed to build.

    cargo build --release

The binary speaks MCP over stdio. It is meant to be launched by an MCP client
(Claude Desktop, Cursor, VS Code, ...), for example:

    npx @modelcontextprotocol/inspector target/release/blender-mcp-rs

The Blender addon must be installed and its server started (or launched via
`xvfb-run -a blender` for headless use).

## Setup

See [docs/setup.md](docs/setup.md) for the controlled setup and verification
procedure (DOC-BMR-SETUP-001): installing Blender into `~/bin`, building the
server, launching it with the addon (GUI and headless), wiring up an MCP
client such as Claude Desktop or Open Grok, the live check with acceptance
criteria, known limitations, and the environment variables the server honors
(`BLENDER_HOST`, `BLENDER_PORT`, `RUST_LOG`).

The project also ships a Grok skill at `.opengrok/skills/blender-mcp/` that
documents the same workflow for AI agents, including every tool and its
parameters.

## Testing

    cargo test

The suite covers:

- Unit tests for the `bbox_condition` normalizer (`_process_bbox` port).
- Wire-protocol tests against an in-process mock of the addon socket server:
  request framing, error statuses, chunked responses, concurrent pairing,
  reconnect on dead sockets, garbage responses, and timeouts.
- Tool-level tests asserting the exact 1:1 formatted output strings for every
  tool against the mock server, including the exact params forwarded to the
  addon (proving `user_prompt` and other MCP-layer fields never leak into the
  addon payload).
- An end-to-end integration test (`tests/blender_integration.rs`) that boots
  the real addon inside a headless local Blender and drives it over TCP. It
  locates Blender via `BLENDER_BIN`, then the known `~/bin` path, then
  `blender` on PATH, and skips with a notice when no Blender binary is
  present.

## Live test (windowed Blender)

To verify the real production path in a GUI Blender session (the addon's
`bpy.app.timers` only fire in windowed mode):

    blender --python tests/live/start_live.py -- <abs/path/to/addon.py>
    cargo run --example live_check -- 9876

`start_live.py` registers the addon (auto-starting the socket server on port
9876), enables all integrations, and adds a test cube. `live_check` connects
through the same wire client the tools use and asserts scene info, object
info, the unknown-object error path, code execution, and all four integration
status tools; it exits 0 with `LIVE_CHECK_PASS` only when every check passes.

During development, the driver also serves multiple commands per connection,
exactly like the addon's real `_handle_client` loop, so the reused-socket
behavior of the client is exercised for real.

## Project layout

- `src/server.rs` - MCP tool/prompt/router wiring (`ServerHandler`)
- `src/connection.rs` - `BlenderConnection` socket client with framing
- `src/tools/` - one module per integration area (scene, polyhaven, sketchfab,
  hyper3d, hunyuan), each a 1:1 port of the corresponding Python tool
- `src/bbox.rs` - the `_process_bbox` helper
- `src/util.rs` - shared formatting/truthiness helpers
- `addon/addon.py` - the original Blender addon (must stay Python)
- `tests/` - mock-server tests and the real-Blender integration test
- `tests/live/` - the windowed live-test launcher
- `examples/live_check.rs` - the live-test verification client
- `docs/setup.md` - build and setup guide
- `.opengrok/skills/blender-mcp/` - the Grok skill for AI agents

## Documentation

Every public item carries a doc comment (`#![warn(missing_docs)]`), so the
API reference is complete. Generate it with:

    cargo doc --open

## License

MIT, matching the original project.
