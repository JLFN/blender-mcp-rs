# Verification guide

How the test suites work, and how to verify a release build before shipping.

## Test layers

1. Library unit tests (`src/`, 9 tests) — the `bbox_condition` normalizer
   (`_process_bbox` port) and shared formatting/truthiness helpers.

2. Wire-protocol tests (`tests/connection_tests.rs`, 13 tests) — the
   `BlenderConnection` socket client against an in-process mock of the addon
   server: request framing, error statuses, chunked responses, concurrent
   command/response pairing under the single mutex, reconnect on dead
   sockets, garbage responses, and timeouts.

3. Tool-level tests (`tests/tools_tests.rs`, 27 tests) — for every one of
   the 22 tools, the exact 1:1 formatted output string is asserted against
   the mock server, including the exact params forwarded to the addon
   (proving `user_prompt` and other MCP-layer fields never leak into the
   addon payload).

4. End-to-end integration test (`tests/blender_integration.rs`, 1 test) —
   boots the real addon inside a headless local Blender and drives it over
   TCP. It locates Blender via `BLENDER_BIN`, then the known `~/bin` path,
   then `blender` on PATH, and skips with a notice when no Blender binary is
   present.

5. Live check (manual / windowed) — the production path through a real GUI
   Blender session, documented in docs/setup.md section 8 and run with:

   ```console
   cargo run --example live_check -- 9876
   ```

## Running everything

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The crate also enforces `#![warn(missing_docs)]`; `cargo doc --no-deps`
must build with zero warnings.

## Verifying a release build

```console
bash build/linux/build.sh -p .
ls -la bin/blender-mcp-rs
```

Smoke-test the built binary directly over stdio: send an `initialize`
request and expect a JSON-RPC result back:

```console
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0.0.1"}}}\n' \
  | bin/blender-mcp-rs
```

Then list the tools over stdio:

```console
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"0.1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | bin/blender-mcp-rs
```

Expect 22 tools named `blender_*` plus the `asset_creation_strategy`
prompt. Finally, confirm the server registers cleanly with Open Grok:

```console
open-grok mcp doctor blender
```

## Live acceptance criteria

With a windowed Blender running the addon on port 9876, the live check must
exit 0 and print `LIVE_CHECK_PASS` only when all of the following hold:

1. `get_scene_info` returns the live scene containing the test cube at
   (1,2,3).
2. `get_object_info` returns MESH topology (vertex count) for the cube.
3. An unknown object returns the documented error path ("Object not
   found").
4. `execute_blender_code` round-trips output through the GUI timer hop.
5. All four integration status tools respond; PolyHaven and Hunyuan3D
   report enabled (no credentials required), Sketchfab reports enabled only
   when a valid API key is stored, Hyper3D reports disabled until an API
   key is set.

## Badge verification

The README carries the license and github badges (crates.io and docs.rs
badges are intentionally omitted until the crate is published). Check each
shields.io URL returns an SVG that does not contain "badge not found":

```console
curl -sL 'https://img.shields.io/badge/license-MIT-97ca00' | grep -c 'badge not found' || true
curl -sL 'https://img.shields.io/badge/github-JLFN_blender_mcp_rs-8da0cb' | grep -c 'badge not found' || true
```
