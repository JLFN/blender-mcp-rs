# Building and setting up blender-mcp-rs

This guide covers installing Blender, building the Rust MCP server, launching
Blender with the addon, connecting an MCP client, and verifying everything
works. It matches the setup on this machine but works on any Linux box with a
local Blender.

## 1. Install Blender

Download the Linux build of Blender and unpack it into your `~/bin` folder so
the `blender` command is on PATH:

    mkdir -p ~/bin
    tar -xf blender-5.x.x-linux-x64.tar.xz -C ~/bin
    ln -sf ~/bin/blender-5.x.x-linux-x64/blender ~/bin/blender
    blender --version

Requirements: a display server (X11 or Wayland) for GUI use. Headless runs
(`blender -b`) work for tests but the addon's command timer never fires
there, so real usage needs a window (or `xvfb-run -a blender`).

## 2. Build the Rust server

Requires a stable Rust toolchain (edition 2021). No Blender needed to build.

    cd blender-mcp-rs
    cargo build --release

The binary is `target/release/blender-mcp-rs`. It speaks MCP over stdio and
is meant to be launched by an MCP client, not run directly in a terminal.

## 3. Launch Blender with the addon

Clone or copy this repo so `addon/addon.py` is available, then start a
windowed Blender with the addon loaded and its socket server auto-started:

    blender --python tests/live/start_live.py -- <absolute/path/to/addon.py>

The addon binds a TCP JSON socket on `127.0.0.1:9876` (the scene property
`blendermcp_port`). The launch script enables all four integrations
(PolyHaven, Sketchfab, Hyper3D, Hunyuan3D) and adds a test cube at (1,2,3).
Wait for `LIVE_READY port=9876` in the log.

You can also install the addon normally in Blender (Edit > Preferences >
Add-ons > Install from Disk) and tick "Auto-Start Server" in the addon
preferences; the sidebar panel (press N) shows the server controls and lets
you set API keys and the port.

## 4. Configure an MCP client

Point your MCP client at the built binary with stdio transport, for example
in Claude Desktop's `claude_desktop_config.json`:

    {
      "mcpServers": {
        "blender": {
          "command": "/home/<user>/blender-mcp-rs/target/release/blender-mcp-rs",
          "args": []
        }
      }
    }

Registering in Grok (Open Grok TUI):

    open-grok mcp add blender -- /data/blender-mcp-rs/target/release/blender-mcp-rs
    open-grok mcp doctor blender
    open-grok mcp list

Tools are namespaced `blender__<tool>` (for example `blender__get_scene_info`).
A server added after the session started needs a refresh (`/mcps`, press `r`)
or a session restart before it appears.

The server announces 22 tools plus the `asset_creation_strategy` prompt.
Environment variables honored: `BLENDER_HOST` (default `localhost`) and
`BLENDER_PORT` (default `9876`) for the addon socket, and `RUST_LOG` (default
`info`) for logging to stderr. The client resolves the host across IPv4 and
IPv6 exactly like Python's `socket.create_connection`, so the addon may bind
either family.

## 5. Verify it works

Run the live check against a running Blender + addon:

    cd blender-mcp-rs
    cargo run --example live_check -- 9876

Exit code 0 and `LIVE_CHECK_PASS` mean scene info, object info, the
unknown-object error path, code execution, and all four integration status
tools respond correctly.

Automated tests (mock server + headless real Blender):

    cargo test

The integration test finds Blender via `BLENDER_BIN`, then
`~/bin/blender-5.1.2-linux-x64/blender`, then `blender` on PATH, and skips
with a notice when no Blender is installed.

## 6. Documentation

Generate the rustdoc API reference:

    cargo doc --open

The crate enforces `#![warn(missing_docs)]`, so every public item carries a
doc comment and the generated docs are complete.

## Troubleshooting

- "cannot start server in background mode": you launched with `blender -b`.
  Use a windowed launch or the headless test driver.
- Addon answers "unexpected keyword argument": a tool forwarded a param the
  addon handler does not take. Only the documented params are ever sent.
- Port 9876 in use: stop the other Blender/server or change
  `blendermcp_port` in the scene (and set the same port in the client).
- Screenshots come back black: the Rust tool renders the viewport offscreen
  (GPU-independent); if the GPU context is unavailable it reports an error
  instead of returning a black image.
