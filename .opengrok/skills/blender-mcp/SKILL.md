---
name: blender-mcp
description: >
  Control Blender through the blender-mcp-rs MCP server: start Blender with the
  BlenderMCP addon (GUI or headless), inspect scenes and objects, execute
  Python inside Blender, capture viewport screenshots, and import assets from
  PolyHaven, Sketchfab, Hyper3D Rodin, and Hunyuan3D. Use when the user wants
  to create or modify 3D content in Blender, query the scene, run bpy code,
  download models/textures/HDRI environments, or when the user runs
  /blender-mcp. Includes how to launch Blender with MCP, the full tool list,
  and the live end-to-end check.
---

# Blender MCP (blender-mcp-rs)

This skill lets Grok drive a local Blender through the Rust MCP server
`blender-mcp-rs` (a 1:1 port of the Python BlenderMCP server, telemetry
removed). Blender itself is controlled by a Python addon that runs inside it;
the Rust binary is the MCP bridge between Grok and that addon.

## System layout (this machine)

- Blender installation: `/home/leandro/bin/blender-5.1.2-linux-x64/`
  (binary at `.../blender`, launched as `blender` from PATH via the symlink
  `/home/leandro/bin/blender`)
- Rust repo: `/data/blender-mcp-rs` (library crate + `target/` build)
- Addon (must stay Python): `/data/blender-mcp-rs/addon/addon.py`
- Live-test launcher: `/data/blender-mcp-rs/tests/live/start_live.py`
- Live-check client: `/data/blender-mcp-rs/examples/live_check.rs`
- MCP socket: the addon listens on `127.0.0.1:9876` by default (scene
  property `blendermcp_port`)
- Desktop shortcuts: `~/.local/share/applications/Blender.desktop`,
  `~/.local/share/applications/blender.desktop`, and the desktop copies under
  `~/Desktop/`, all pointing at `/home/leandro/bin/blender-5.1.2-linux-x64/`

## How the pieces talk

1. Blender runs the addon, which opens a TCP JSON socket server.
2. The Rust MCP server (`/data/blender-mcp-rs`) connects to that socket and
   sends `{"type": <command>, "params": {...}}`.
3. The addon executes the command on Blender's main thread (via
   `bpy.app.timers` in GUI mode) and replies
   `{"status": "success", "result": ...}` or `{"status": "error", "message": ...}`.
4. Responses are matched to commands purely by stream order; the client holds
   one lock across send and receive, and reconnects a dead socket on the next
   command.

Wire facts to remember: params serialize as `{}` when empty; the addon rejects
unknown params, so only ever send the documented tool params (never forward
`user_prompt`-style extras to the addon). The default response timeout is 180s.

## Starting Blender with MCP

GUI (production path, timers fire):

    blender --python /data/blender-mcp-rs/tests/live/start_live.py -- /data/blender-mcp-rs/addon/addon.py

This opens a Blender window, registers the addon (auto-starting the server on
port 9876), enables all four integrations, and adds a test cube at (1,2,3).
Wait for `LIVE_READY port=9876` in the log.

Headless (for scripting/tests, no window): use the test driver instead of the
addon's own server, because `bpy.app.timers` never fire in `blender -b`:

    blender -b -P /data/blender-mcp-rs/tests/driver/blender_test_driver.py -- /data/blender-mcp-rs/addon/addon.py 9876

The driver calls `execute_command` directly on the main thread and serves
multiple commands per connection, mirroring the addon's real `_handle_client`.

If the port is taken or Blender does not print the ready line, check the
launch log and the `ss -tlnp` output before retrying.

## Registering the MCP server in Grok

The Rust server must be registered as an MCP server before Grok can call its
tools. It is already registered on this machine as `blender` (user scope,
`~/.opengrok/config.toml`):

    [mcp_servers.blender]
    command = "/data/blender-mcp-rs/target/release/blender-mcp-rs"
    args = []
    enabled = true

To register (or re-register) it from the CLI:

    open-grok mcp add blender -- /data/blender-mcp-rs/target/release/blender-mcp-rs
    open-grok mcp doctor blender       # handshake + tool discovery check
    open-grok mcp list                 # shows all servers, incl. blender

Tool name prefix: every tool is namespaced as `blender__<tool>`, for example
`blender__get_scene_info`, `blender__execute_blender_code`.

If the server was added after the Grok session started, the running session
will not see it until you press `r` in the `/mcps` modal (or Ctrl+L, MCP
Servers tab) to refresh, or restart the session. The server connects to
Blender on port 9876 via `BLENDER_HOST`/`BLENDER_PORT` (defaults
`localhost`/`9876`); it resolves the host like Python's `create_connection`
and falls back across IPv4/IPv6, so Blender must be listening on the port.

## Verifying it works (live check)

With a GUI Blender + addon running on port 9876:

    cd /data/blender-mcp-rs && cargo run --example live_check -- 9876

Exit code 0 and `LIVE_CHECK_PASS` mean: scene info (finds Cube.001), object
info (MESH, 8 vertices, location 1,2,3), the "Object not found" error path,
`execute_code` round-trip, and all four status tools responding.

## Building and testing the Rust server

    cd /data/blender-mcp-rs
    cargo build --release            # no Blender needed to build
    cargo test                       # 49 tests: unit, mock-server wire tests,
                                     # per-tool output tests, real-Blender headless
                                     # integration test (skips if no Blender)

The integration test finds Blender via `BLENDER_BIN`, else
`/home/leandro/bin/blender-5.1.2-linux-x64/blender`, else `blender` on PATH.

## MCP tools (all 22)

Scene / code:
- get_scene_info: objects, locations, counts of the current scene
- get_object_info(object_name): type, location/rotation/scale, visibility,
  world bounding box, material slots, mesh vertex/edge/polygon counts
- get_viewport_screenshot(max_size=1000): renders the 3D viewport offscreen
  and returns a PNG image (works even when the window is not composited)
- execute_blender_code(code): runs arbitrary Python (bpy) step by step in
  Blender; returns captured stdout

PolyHaven (assets: hdris / textures / models):
- get_polyhaven_status
- get_polyhaven_categories(asset_type="hdris")
- search_polyhaven_assets(asset_type="all", categories)
- download_polyhaven_asset(asset_id, asset_type, resolution="1k", file_format)
- set_texture(object_name, texture_id)

Hyper3D Rodin (generation):
- get_hyper3d_status
- generate_hyper3d_model_via_text(text_prompt, bbox_condition)
- generate_hyper3d_model_via_images(input_image_paths | input_image_urls,
  bbox_condition)  (give exactly one of the two image inputs)
- poll_rodin_job_status(subscription_key | request_id)
- import_generated_asset(name, task_uuid | request_id)

Sketchfab (models):
- get_sketchfab_status
- search_sketchfab_models(query, categories, count=20, downloadable=true)
- get_sketchfab_model_preview(uid): returns a thumbnail image
- download_sketchfab_model(uid, target_size): scales largest dimension to
  target_size meters

Hunyuan3D (generation):
- get_hunyuan3d_status
- generate_hunyuan3d_model(text_prompt | input_image_url)
- poll_hunyuan_job_status(job_id)
- import_generated_asset_hunyuan(name, zip_file_url)

Plus the asset_creation_strategy prompt, which tells agents how to pick
between the libraries (Sketchfab for realistic/specific models, PolyHaven for
generic objects and HDRIs/textures, Hyper3D/Hunyuan3D for custom items).

## Integration statuses on this machine

- PolyHaven: enabled (checkbox only; no key required)
- Hyper3D: disabled until an API key is set in the sidebar panel
- Sketchfab: enabled; a real API key is configured (validated live as user
  JLFN82)
- Hunyuan3D: enabled in LOCAL_API mode (no credentials required)

## Workflow guidance

1. Start by calling get_scene_info to see what is in the scene.
2. Take a get_viewport_screenshot before and after changes to verify visually.
3. For assets: check the integration status first, then search, preview
   (Sketchfab), download, and verify with get_object_info / screenshots.
4. When generating (Hyper3D/Hunyuan3D): create the job, poll until
   done/failed, import, then check the world_bounding_box and reposition/scale
   the imported mesh so it sits correctly in the scene.
5. Fall back to execute_blender_code only when libraries are disabled or the
   task needs a primitive/material that libraries cannot provide; always break
   code into small, verifiable steps.
6. Never send parameters the addon does not accept; extra keys cause a
   "TypeError: ... unexpected keyword argument" from the addon.

## Troubleshooting

- "cannot start server in background mode": you launched with `blender -b`.
  Use the GUI launch above or the headless test driver.
- EOF / "Connection to Blender lost" mid-session: the addon closed the
  socket; the client reconnects on the next command automatically.
- Addon replies "unexpected keyword argument": you forwarded a param the addon
  handler does not take (for example user_prompt). Send only documented params.
- Port already in use: another Blender instance or server holds 9876; stop it
  or change `blendermcp_port` in the scene.
- Screenshot all black: run with the offscreen render path (the Rust tool
  already uses it); do not rely on window compositing.
