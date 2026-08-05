//! Live-check client: verifies a running BlenderMCP addon server end to end.
//!
//! This is the verification half of the live test. It connects to the addon's
//! TCP socket (the exact same wire client the MCP tools use) and runs the
//! core command set against a real, windowed Blender session:
//!
//!     1. Start Blender with the addon loaded:
//!            blender --python tests/live/start_live.py -- <path/to/addon.py>
//!     2. Run this check:
//!            cargo run --example live_check -- <port>
//!
//! Exit code 0 means every check passed.

use std::time::Duration;

use serde_json::json;

use blender_mcp_rs::connection::BlenderConnection;

fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(9876);

    let mut c = BlenderConnection::new("127.0.0.1", port);
    c.set_timeout(Duration::from_secs(30));

    let mut failures = 0usize;

    // 1. Scene info: the cube the launch script created is present.
    match c.send_command("get_scene_info", None) {
        Ok(scene) => {
            let objects = scene.get("objects").and_then(|o| o.as_array()).unwrap();
            let names: Vec<&str> = objects
                .iter()
                .filter_map(|o| o.get("name").and_then(|n| n.as_str()))
                .collect();
            println!("scene_info: {} objects, names = {names:?}", objects.len());
            if !names.iter().any(|n| *n == "Cube.001") {
                println!("  FAIL: Cube.001 not found");
                failures += 1;
            }
        }
        Err(e) => {
            println!("  FAIL: get_scene_info: {e}");
            failures += 1;
        }
    }

    // 2. Object info: mesh detail for the cube.
    match c.send_command("get_object_info", json!({ "name": "Cube.001" }).as_object()) {
        Ok(obj) => {
            let t = obj.get("type").and_then(|t| t.as_str()).unwrap_or("?");
            let verts = obj
                .get("mesh")
                .and_then(|m| m.get("vertices"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let loc = obj.get("location").cloned().unwrap_or(json!([]));
            println!("object_info: type={t} vertices={verts} location={loc}");
            if t != "MESH" || verts != 8 {
                println!("  FAIL: unexpected object info");
                failures += 1;
            }
        }
        Err(e) => {
            println!("  FAIL: get_object_info: {e}");
            failures += 1;
        }
    }

    // 3. Error path: an unknown object surfaces the addon's message.
    match c.send_command("get_object_info", json!({ "name": "NOT_THERE" }).as_object()) {
        Ok(_) => {
            println!("  FAIL: unknown object did not error");
            failures += 1;
        }
        Err(e) => {
            println!("error_path: {e}");
            if !e.message.contains("Object not found") {
                println!("  FAIL: unexpected error text");
                failures += 1;
            }
        }
    }

    // 4. Code execution round-trips stdout through the timer hop.
    match c.send_command(
        "execute_code",
        json!({ "code": "import bpy\nprint('LIVE_OK')\n" }).as_object(),
    ) {
        Ok(result) => {
            let out = result.get("result").and_then(|r| r.as_str()).unwrap_or("");
            println!("execute_code: {out:?}");
            if out != "LIVE_OK\n" {
                println!("  FAIL: unexpected code output");
                failures += 1;
            }
        }
        Err(e) => {
            println!("  FAIL: execute_code: {e}");
            failures += 1;
        }
    }

    // 5. Integrations: all four status commands answer (PolyHaven is
    // checkbox-only; the others need credentials, so only check they respond).
    for cmd in [
        "get_polyhaven_status",
        "get_hyper3d_status",
        "get_sketchfab_status",
        "get_hunyuan3d_status",
    ] {
        match c.send_command(cmd, None) {
            Ok(r) => println!("{cmd}: ok {r:?}"),
            Err(e) => {
                println!("  FAIL: {cmd}: {e}");
                failures += 1;
            }
        }
    }

    if failures == 0 {
        println!("LIVE_CHECK_PASS");
    } else {
        println!("LIVE_CHECK_FAIL ({failures} failures)");
        std::process::exit(1);
    }
}
