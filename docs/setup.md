# blender-mcp-rs — Setup and Verification Procedure

| Item | Value |
| --- | --- |
| Document ID | DOC-BMR-SETUP-001 |
| Status | Released |
| Version | 2.0 |
| Last reviewed | 2026-08-05 |
| Applicable to | blender-mcp-rs v1.8.0 (all platforms with a local Blender) |

## 0. Purpose and Scope

This document defines the controlled procedure for installing, building,
configuring, and verifying the blender-mcp-rs MCP server and its companion
Blender addon. It is the normative how-to for this repository.

In scope:

1. Installation of the Blender runtime into the user bin directory.
2. Compilation of the Rust MCP server binary.
3. Launch of Blender with the addon in GUI and headless modes.
4. Registration of the server with an MCP client (Claude Desktop, Open Grok).
5. Verification of the full chain (client to addon to Blender) and acceptance
   criteria.

Out of scope: Blender addon development, non-Linux platforms, and the
original Python server (removed; see Known Limitations).

## 1. References

| Reference | Description |
| --- | --- |
| RFC 2119 | Key words for requirements levels (MUST, SHOULD, MAY) |
| MCP specification | Model Context Protocol, stdio transport |
| Cargo book | Rust build system, edition 2021 |
| Original project | ahujasid/blender-mcp (upstream Python reference) |

## 2. Definitions and Abbreviations

| Term | Meaning |
| --- | --- |
| Addon | addon/addon.py, the Python bridge that runs inside Blender |
| Server | The Rust binary target/release/blender-mcp-rs |
| Addon socket | TCP JSON socket bound by the addon, default 127.0.0.1:9876 |
| MCP client | Any program speaking MCP over stdio (Claude, Grok, ...) |

## 3. Prerequisites

Before starting, confirm the following. Each MUST hold:

1. A Linux host with a display server (X11 or Wayland) for GUI operation.
   Headless operation requires a window or xvfb-run.
2. A stable Rust toolchain supporting edition 2021 (cargo 1.97 or newer).
3. Network access to download Blender (https://www.blender.org/download/)
   and to fetch crates from crates.io.
4. The repository checked out at /data/blender-mcp-rs (this machine) or the
   equivalent path on the target host.
5. No process currently bound to the addon port 9876, unless reusing an
   existing live session.

Verification of prerequisites:

    blender --version        # only if already installed; see section 4
    cargo --version          # must report edition-2021-capable toolchain

## 4. Procedure 1 — Install Blender into the user bin folder

Purpose: place the Blender runtime in ~/bin so the blender command is on
PATH and desktop shortcuts resolve to a stable location.

Steps:

1. Download the Linux archive (for example blender-5.1.2-linux-x64.tar.xz).
2. Unpack into ~/bin:

       mkdir -p ~/bin
       tar -xf blender-5.1.2-linux-x64.tar.xz -C ~/bin

3. Create the PATH symlink:

       ln -sf ~/bin/blender-5.1.2-linux-x64/blender ~/bin/blender

4. Update desktop launchers that referenced the previous install location
   (~/.local/share/applications/Blender.desktop and blender.desktop, plus any
   copies on the Desktop): point both Exec and Icon entries at the new path.

Verification (all MUST pass):

    which blender          # resolves under ~/bin
    blender --version      # reports the expected 5.1.2 build
    grep -rl 'blender-5' ~/.local/share/applications/*.desktop  # new path only
    grep -rl '/home/leandro/Documents/blender' ~/.local/share/applications/*.desktop
                           # must produce no output (old path gone)

## 5. Procedure 2 — Build the Rust server

Purpose: produce the stdio MCP server binary. No Blender installation is
required to build.

Steps:

    cd /data/blender-mcp-rs
    cargo build --release

Verification (all MUST pass):

1. cargo reports a successful build with no warnings.
2. target/release/blender-mcp-rs exists and is executable.
3. cargo doc --no-deps builds with zero warnings (the crate enforces
   #![warn(missing_docs)]).
4. cargo test passes all 50 tests (mock-server wire tests, per-tool output
   tests, bbox unit tests, and the headless Blender integration test).

The binary speaks MCP over stdio only; it is NOT meant to be executed
directly in a terminal. It will idle on stdin awaiting a client handshake.

## 6. Procedure 3 — Launch Blender with the addon (GUI, production path)

Purpose: start a windowed Blender session with the addon registered and its
socket server auto-started. This is the production path: bpy.app.timers fire
only in windowed mode, so commands execute on the main thread exactly as they
do in normal interactive use.

Steps:

    blender --python tests/live/start_live.py -- /data/blender-mcp-rs/addon/addon.py

The launcher registers the addon (auto-starting the socket server on port
9876, controlled by the scene property blendermcp_port), enables all four
integrations (PolyHaven, Sketchfab, Hyper3D, Hunyuan3D), and adds a test cube
at (1,2,3).

Verification:

1. The Blender window opens on the display.
2. The log prints LIVE_READY port=9876.
3. Optionally install the addon through Edit > Preferences > Add-ons >
   Install from Disk and tick Auto-Start Server; the sidebar panel (press N)
   exposes server controls and API key fields.

Headless variant (test path only): blender -b -P tests/driver/
blender_test_driver.py -- <addon path> <port>. The driver mirrors the
addon's real _handle_client loop (one connection serves many commands), which
exercises the reused-socket behavior of the client.

## 7. Procedure 4 — Register the server with an MCP client

Purpose: make the 22 tools discoverable to an MCP client over stdio.

Claude Desktop — claude_desktop_config.json:

    {
      "mcpServers": {
        "blender": {
          "command": "/home/<user>/blender-mcp-rs/target/release/blender-mcp-rs",
          "args": []
        }
      }
    }

Open Grok (TUI):

    open-grok mcp add blender -- /data/blender-mcp-rs/target/release/blender-mcp-rs
    open-grok mcp doctor blender
    open-grok mcp list

Verification (all MUST pass):

1. open-grok mcp doctor blender reports handshake OK and 22 tools.
2. Tools are namespaced blender__<tool>, for example blender__get_scene_info
   and blender__execute_blender_code.
3. A server registered after a session started requires a refresh (/mcps,
   press r) or a session restart before it appears in that session.

Environment variables honored by the server (all optional): BLENDER_HOST
(default localhost), BLENDER_PORT (default 9876) for the addon socket, and
RUST_LOG (default info) for stderr logging. The client resolves the host
across IPv4 and IPv6 in order, exactly like Python's socket.create_connection,
so the addon may bind either address family.

## 8. Procedure 5 — Live verification and acceptance criteria

Purpose: prove the full chain works against a real, running Blender.

Steps:

    cd /data/blender-mcp-rs
    cargo run --example live_check -- 9876

Acceptance criteria — the live check exits 0 and prints LIVE_CHECK_PASS only
when ALL of the following hold:

1. get_scene_info returns the live scene containing the test cube at (1,2,3).
2. get_object_info returns MESH topology (vertex count) for the cube.
3. An unknown object returns the documented error path ("Object not found").
4. execute_blender_code round-trips output through the GUI timer hop.
5. All four integration status tools respond; PolyHaven and Hunyuan3D report
   enabled (no credentials required), Sketchfab reports enabled only when a
   valid API key is stored, Hyper3D reports disabled until an API key is set.

Automated regression: cargo test runs the same assertions headless against a
mock server and a real headless Blender. The integration test locates Blender
via BLENDER_BIN, then ~/bin/blender-5.1.2-linux-x64/blender, then blender on
PATH; it skips with a notice when no Blender is found.

## 9. Known Limitations

1. The addon cannot be ported to Rust: it drives bpy from inside Blender's
   Python interpreter. It ships unmodified.
2. bpy.app.timers never fire in headless mode, so real usage requires a
   window or xvfb-run; headless is for testing only.
3. Telemetry from the original server is removed by design and is not
   re-implemented; no metrics leave the process.
4. The Hyper3D image-URL branch validates the URL list (the Python original
   validated a variable that is always None, so that branch always failed).
   This is an intentional documented divergence.
5. One cosmetic f64 formatting difference exists in a Sketchfab download line
   (2 vs 2.0); output content is otherwise byte-identical to the original.
6. Screenshots render offscreen (GPU-independent); if no GPU context is
   available the tool reports an error instead of returning a black image.

## 10. Troubleshooting

| Symptom | Cause | Resolution |
| --- | --- | --- |
| "cannot start server in background mode" | Launched with blender -b | Use windowed launch or the headless driver |
| Addon answers "unexpected keyword argument" | A tool forwarded an extra param | Only documented params are ever sent; file a bug |
| Port 9876 in use | Another Blender/server holds it | Stop it, or change blendermcp_port and mirror it in the client |
| Black screenshots | GPU context unavailable | Offscreen rendering; expected without a usable context |
| Tools missing in Grok session | Server registered after session start | /mcps, press r, or restart the session |

## 11. Revision History

| Version | Date | Change |
| --- | --- | --- |
| 1.0 | 2026-08-05 | Initial release of the guide |
| 2.0 | 2026-08-05 | Restructured to controlled document format (this revision) |
