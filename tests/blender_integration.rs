//! End-to-end integration test that drives the real addon inside a headless
//! local Blender (`blender -b`) via the mock-free `BlenderConnection`.
//!
//! This is the test the user asked for: "all the tests we have a local blender
//! installed". It locates a Blender binary via `BLENDER_BIN` (falling back to a
//! well-known local path), launches it headless with the test driver, waits for
//! the driver to become ready, then exercises the real command handlers.
//!
//! When no Blender binary is available the test prints a notice and returns
//! early (skips) so `cargo test` is not red on machines without Blender, but it
//! runs in full on any machine with Blender installed.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::json;

use blender_mcp_rs::connection::BlenderConnection;

const DEFAULT_BLENDER: &str =
    "/home/leandro/Documents/blender-5.1.2-linux-x64/blender";

fn blender_bin() -> Option<String> {
    if let Ok(path) = std::env::var("BLENDER_BIN") {
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }
    if std::path::Path::new(DEFAULT_BLENDER).exists() {
        return Some(DEFAULT_BLENDER.to_string());
    }
    None
}

/// Ephemeral free TCP port for the addon socket.
fn ephemeral_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

/// Kills the Blender child on drop, even when a test assertion panics.
struct BlenderGuard {
    child: Child,
    _reader: thread::JoinHandle<()>,
}

impl Drop for BlenderGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // std thread handle is not detached; joining a thread that exits on
        // reader EOF is fine, but we intentionally don't join to avoid hangs.
    }
}

/// Whether a line announces the driver is ready to accept commands.
fn is_ready(line: &str) -> bool {
    line.contains("DRIVER_READY")
}

/// Spawn Blender headless with the test driver and wait until it is ready.
fn spawn_blender(port: u16) -> (BlenderGuard, mpsc::Receiver<String>) {
    let bin = blender_bin().expect("integration test requires a local Blender binary");
    let driver = std::env::var("CARGO_MANIFEST_DIR")
        .map(|d| format!("{d}/tests/driver/blender_test_driver.py"))
        .unwrap_or_else(|_| "tests/driver/blender_test_driver.py".to_string());
    let addon = std::env::var("CARGO_MANIFEST_DIR")
        .map(|d| format!("{d}/addon/addon.py"))
        .unwrap_or_else(|_| "addon/addon.py".to_string());

    let mut child = Command::new(&bin)
        .arg("-b")
        .arg("-P")
        .arg(&driver)
        .arg("--")
        .arg(&addon)
        .arg(port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to launch Blender {bin}: {e}"));

    let stdout = child.stdout.take().expect("child stdout");
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let rd = BufReader::new(stdout);
        for line in rd.lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    (BlenderGuard { child, _reader: reader }, rx)
}

fn wait_ready(rx: &mpsc::Receiver<String>) -> Option<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(line) => {
                let ready = is_ready(&line);
                last = line;
                if ready {
                    return Some(last);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    eprintln!("Blender integration: never became ready; last line: {last:?}");
    None
}

#[test]
fn real_blender_end_to_end() {
    let Some(_bin) = blender_bin() else {
        eprintln!(
            "SKIP real_blender_end_to_end: no local Blender binary found \
             (set BLENDER_BIN or install to a known path)"
        );
        return;
    };

    let port = ephemeral_port();
    let (guard, rx) = spawn_blender(port);
    assert!(
        wait_ready(&rx).is_some(),
        "driver did not become ready within 120s; Blender startup may have failed"
    );

    // The connection keeps the socket open across commands, so this also
    // exercises multi-command-per-connection serving by the driver.
    let mut c = BlenderConnection::new("127.0.0.1", port);
    c.set_timeout(Duration::from_secs(60));

    // 1. Scene info contains the cube the driver created at (1, 2, 3).
    let scene = c
        .send_command("get_scene_info", None)
        .expect("get_scene_info should succeed");
    let objects = scene.get("objects").and_then(|o| o.as_array()).unwrap();
    let cube = objects
        .iter()
        .find(|o| o.get("name").and_then(|n| n.as_str()) == Some("Cube.001"))
        .unwrap_or_else(|| panic!("Cube.001 missing from scene: {scene:?}"));
    assert_eq!(
        cube.get("location").cloned(),
        Some(json!([1.0, 2.0, 3.0])),
        "cube location"
    );

    // 2. Detailed mesh info for the cube.
    let obj = c
        .send_command(
            "get_object_info",
            json!({ "name": "Cube.001" }).as_object(),
        )
        .expect("get_object_info should succeed");
    assert_eq!(obj.get("type").and_then(|t| t.as_str()), Some("MESH"));
    assert_eq!(
        obj.get("mesh")
            .and_then(|m| m.get("vertices"))
            .and_then(|v| v.as_i64()),
        Some(8)
    );

    // 3. Unknown object surfaces the addon's error.
    let err = c
        .send_command("get_object_info", json!({ "name": "NOT_THERE" }).as_object())
        .expect_err("unknown object must error");
    assert!(
        err.message.contains("Object not found"),
        "unexpected error: {}",
        err.message
    );

    // 4. Raw code execution round-trips stdout.
    let code_result = c
        .send_command("execute_code", json!({ "code": "import bpy\nprint('INTEGRATION_OK')\n" }).as_object())
        .expect("execute_code should succeed");
    assert_eq!(
        code_result.get("result").and_then(|r| r.as_str()),
        Some("INTEGRATION_OK\n")
    );

    // 5. Status tools: PolyHaven only needs the checkbox (enabled true); the
    // others require API credentials which aren't set in headless testing, so
    // they report enabled false with a descriptive message.
    //
    // These assertions are against the raw addon (no server-augmented hints),
    // which is why "good at Textures" and similar appendixes are absent.
    for (cmd, expect_enabled, expect_message_needle) in [
        ("get_polyhaven_status", Some(true), "enabled and ready to use"),
        (
            "get_hyper3d_status",
            Some(false),
            "API key is not given",
        ),
        (
            "get_sketchfab_status",
            Some(false),
            "API key is not given",
        ),
        // Hunyuan3D defaults to LOCAL_API mode, which needs no credentials.
        ("get_hunyuan3d_status", Some(true), "enabled and ready to use"),
    ] {
        let r = c.send_command(cmd, None).unwrap_or_else(|e| panic!("{cmd}: {e}"));
        assert_eq!(
            r.get("enabled").and_then(|e| e.as_bool()),
            expect_enabled,
            "{cmd} enabled flag: {r:?}"
        );
        let msg = r
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("");
        assert!(
            msg.contains(expect_message_needle),
            "{cmd} message should contain '{expect_message_needle}': {msg}"
        );
    }

    drop(guard);
}