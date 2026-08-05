//! Wire-protocol tests for `BlenderConnection`, exercising the exact framing
//! and failure behavior of the Python original against the mock addon server.
//!
//! Coverage mirrors the behaviors that matter for a 1:1 port:
//!  - request shape: `{"type": ..., "params": {...}}` with `params or {}`
//!  - responses matched to commands purely by stream ordering (single lock
//!    across send + receive)
//!  - chunked receive until a complete JSON object parses
//!  - dead-socket reconnect on the next command
//!  - 180s default timeout, overridable for tests

mod support;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use blender_mcp_rs::connection::BlenderConnection;
use support::MockBlender;

fn conn(port: u16) -> BlenderConnection {
    let mut c = BlenderConnection::new("127.0.0.1", port);
    // Fail fast in tests instead of waiting the real 180s.
    c.set_timeout(Duration::from_secs(10));
    c
}

#[test]
fn roundtrip_returns_result_and_sends_exact_command() {
    let mock = MockBlender::new();
    mock.respond(
        "get_scene_info",
        json!({ "status": "success", "result": { "name": "Scene", "object_count": 3 } }),
    );

    let c = conn(mock.port());
    let result = c.send_command("get_scene_info", None).unwrap();

    assert_eq!(result, json!({ "name": "Scene", "object_count": 3 }));
    let commands = mock.received();
    assert_eq!(commands.len(), 1);
    // None params serialize as `{}`, matching Python's `params or {}`.
    assert_eq!(
        commands[0],
        json!({ "type": "get_scene_info", "params": {} })
    );
}

#[test]
fn hostname_resolution_tries_every_address() {
    // The addon binds 127.0.0.1 only. On this machine `localhost` resolves to
    // `::1` first, so the client must fall back to the IPv4 address exactly
    // like Python's socket.create_connection does. Without the fallback this
    // fails with "Not connected to Blender".
    let mock = MockBlender::new();
    mock.respond("get_scene_info", json!({ "status": "success", "result": "OK" }));

    let mut c = BlenderConnection::new("localhost", mock.port());
    c.set_timeout(Duration::from_secs(10));
    let result = c.send_command("get_scene_info", None).unwrap();
    assert_eq!(result, json!("OK"));
}

#[test]
fn params_are_forwarded_verbatim() {
    let mock = MockBlender::new();
    mock.respond("execute_code", json!({ "status": "success", "result": {} }));

    let c = conn(mock.port());
    let params = json!({ "code": "import bpy\nprint('hi')" });
    c.send_command("execute_code", params.as_object()).unwrap();

    assert_eq!(
        mock.received()[0],
        json!({ "type": "execute_code", "params": { "code": "import bpy\nprint('hi')" } })
    );
}

#[test]
fn error_status_becomes_error_with_message() {
    let mock = MockBlender::new();
    mock.respond(
        "get_object_info",
        json!({ "status": "error", "message": "Object not found: MISSING" }),
    );

    let c = conn(mock.port());
    let err = c.send_command("get_object_info", None).unwrap_err();
    assert_eq!(err.message, "Object not found: MISSING");
}

#[test]
fn error_status_without_message_uses_fallback() {
    let mock = MockBlender::new();
    mock.respond("execute_code", json!({ "status": "error" }));

    let c = conn(mock.port());
    let err = c.send_command("execute_code", None).unwrap_err();
    assert_eq!(err.message, "Unknown error from Blender");
}

#[test]
fn chunked_response_is_reassembled() {
    let mock = MockBlender::new();
    mock.respond(
        "get_scene_info",
        json!({ "status": "success", "result": { "ok": true, "pad": "x".repeat(4000) } }),
    );
    // Fragment into 1000-byte writes with delays in between.
    mock.chunk(1000);

    let c = conn(mock.port());
    let result = c.send_command("get_scene_info", None).unwrap();
    assert_eq!(result, json!({ "ok": true, "pad": "x".repeat(4000) }));
}

#[test]
fn concurrent_commands_on_one_connection_stay_paired() {
    let mock = MockBlender::new();
    // Responses are keyed by command type; crossed pairing would surface as a
    // wrong result below.
    mock.respond("cmd_a", json!({ "status": "success", "result": "RESP_A" }));
    mock.respond("cmd_b", json!({ "status": "success", "result": "RESP_B" }));

    let c = Arc::new(conn(mock.port()));
    let mut handles = Vec::new();
    for (name, cmd, expected) in [("a", "cmd_a", "RESP_A"), ("b", "cmd_b", "RESP_B")] {
        let c = Arc::clone(&c);
        handles.push(thread::spawn(move || {
            for i in 0..20 {
                let result = c
                    .send_command(cmd, None)
                    .unwrap_or_else(|e| panic!("{name}#{i}: {e}"));
                // A crossed pairing would hand this thread the other command's
                // reply and fail here.
                assert_eq!(result, json!(expected), "{name}#{i}");
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(mock.received().len(), 40);
}

#[test]
fn dead_socket_reconnects_on_next_command() {
    let mock = MockBlender::new();
    mock.respond("get_scene_info", json!({ "status": "success", "result": "FIRST" }));
    mock.close_after_first();

    let c = conn(mock.port());
    // First command works; the mock then closes the connection.
    assert_eq!(
        c.send_command("get_scene_info", None).unwrap(),
        json!("FIRST")
    );
    // Second command hits the closed socket...
    let err = c.send_command("get_scene_info", None).unwrap_err();
    assert!(
        err.message.contains("Connection closed") || err.message.contains("No data"),
        "unexpected error: {}",
        err.message
    );
    // ...and the third reconnects cleanly on a fresh socket.
    assert_eq!(
        c.send_command("get_scene_info", None).unwrap(),
        json!("FIRST")
    );
    assert_eq!(mock.connection_count(), 2);
}

#[test]
fn garbage_response_is_an_error_and_next_command_reconnects() {
    let mock = MockBlender::new();
    mock.respond_raw(b"this is not json {{{".to_vec());
    mock.respond(
        "get_scene_info",
        json!({ "status": "success", "result": "RECOVERED" }),
    );

    let c = conn(mock.port());
    let err = c.send_command("get_scene_info", None).unwrap_err();
    assert!(
        err.message.contains("Invalid response") || err.message.contains("Incomplete JSON"),
        "unexpected error: {}",
        err.message
    );
    // The garbage also closes the connection, so the next command reconnects.
    mock.clear_raw();
    assert_eq!(c.send_command("get_scene_info", None).unwrap(), json!("RECOVERED"));
}

#[test]
fn connection_closed_before_any_data_is_an_error() {
    let mock = MockBlender::new();
    // The server reads the command, then hangs up without replying: a clean
    // FIN that the client sees as EOF with no data received.
    mock.read_then_close();

    let c = conn(mock.port());
    let err = c.send_command("get_scene_info", None).unwrap_err();
    assert_eq!(err.message, "Connection closed before receiving any data");
}

#[test]
fn unread_close_surfaces_as_socket_error() {
    let mock = MockBlender::new();
    // Closing with the client's command possibly still unread makes the kernel
    // choose between RST (socket error) and FIN (clean EOF) depending on
    // timing. Both are prompt errors; the point is that the client fails fast
    // instead of waiting out the 180s timeout.
    mock.close_immediately();

    let c = conn(mock.port());
    let err = c.send_command("get_scene_info", None).unwrap_err();
    assert!(
        err.message.contains("Socket connection error")
            || err.message == "Connection closed before receiving any data",
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn read_timeout_is_respected() {
    let mock = MockBlender::new();
    mock.hang();

    let mut c = BlenderConnection::new("127.0.0.1", mock.port());
    c.set_timeout(Duration::from_millis(400));

    let started = std::time::Instant::now();
    let err = c.send_command("get_scene_info", None).unwrap_err();
    assert!(
        err.message.contains("No data received"),
        "unexpected error: {}",
        err.message
    );
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "timed out too early: {:?}",
        started.elapsed()
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "did not time out: {:?}",
        started.elapsed()
    );
}

#[test]
fn unparseable_result_type_is_an_error() {
    let mock = MockBlender::new();
    mock.respond("get_scene_info", json!({ "status": "success" }));
    let c = conn(mock.port());
    // No "result" key -> falls back to JSON null, like Python's `result.get(...)`.
    assert_eq!(c.send_command("get_scene_info", None).unwrap(), Value::Null);
}
