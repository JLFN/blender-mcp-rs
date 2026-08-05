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
  locates Blender via `BLENDER_BIN` (falling back to a known path) and skips
  with a notice when no Blender binary is present.

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

## License

MIT, matching the original project.
