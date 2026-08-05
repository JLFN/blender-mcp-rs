//! Shared test support: a scriptable mock of the Blender addon's TCP JSON
//! socket server.
//!
//! Compiled into each integration-test binary, so helper methods are used by
//! only some of them; silence per-crate "never used" warnings for the shared
//! surface.
#![allow(dead_code)]
//!
//! The real addon (`addon/addon.py`) accepts a connection and serves any
//! number of commands on it until the client disconnects. `MockBlender`
//! mirrors that contract so the connection layer and every tool function can
//! be exercised without a real Blender process:
//!
//!  - responses are looked up by command type (`{"type": ..., "params": ...}`)
//!  - every received command is recorded for assertion
//!  - responses can be fragmented, replaced with raw bytes, withheld, or the
//!    connection closed to exercise each failure path

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

/// Response the mock writes for a command whose type has no scripted reply.
pub fn default_response() -> Value {
    json!({ "status": "success", "result": null })
}

#[derive(Debug, Default, Clone)]
struct MockConfig {
    /// Per-command-type response bodies, keyed by `type`.
    responses: HashMap<String, Value>,
    /// When set, every command is answered with these raw bytes instead of a
    /// scripted response (used for the garbage-response test).
    raw_reply: Option<Vec<u8>>,
    /// Accept the connection and close it without reading anything.
    close_immediately: bool,
    /// Read the command but never reply (used for the timeout test).
    hang: bool,
    /// Read the command, then close without replying (clean-EOF test).
    read_then_close: bool,
    /// Reply to the first command then close the connection (reconnect test).
    close_after_first: bool,
    /// Fragment every response into writes of this many bytes (0 = one write).
    chunk_bytes: usize,
}

/// A scriptable in-process mock of the Blender addon socket server.
pub struct MockBlender {
    port: u16,
    config: Arc<Mutex<MockConfig>>,
    received: Arc<Mutex<Vec<Value>>>,
    connections: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockBlender {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock listener");
        let port = listener.local_addr().expect("local addr").port();
        let config = Arc::new(Mutex::new(MockConfig::default()));
        let received = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let (cfg, rcvd, conns, stp) =
            (config.clone(), received.clone(), connections.clone(), stop.clone());
        let thread = thread::spawn(move || {
            while !stp.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        conns.fetch_add(1, Ordering::SeqCst);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                        serve_connection(stream, &cfg, &rcvd);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            port,
            config,
            received,
            connections,
            stop,
            thread: Some(thread),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn host(&self) -> &'static str {
        "127.0.0.1"
    }

    /// Script the reply for a given command type.
    pub fn respond(&self, command_type: &str, response: Value) {
        self.config
            .lock()
            .unwrap()
            .responses
            .insert(command_type.to_string(), response);
    }

    /// Reply to every command with raw bytes (overrides scripted replies).
    /// The connection is closed right after the raw bytes are written.
    pub fn respond_raw(&self, bytes: Vec<u8>) {
        self.config.lock().unwrap().raw_reply = Some(bytes);
    }

    /// Clear the raw-reply override so subsequent commands use scripted replies.
    pub fn clear_raw(&self) {
        self.config.lock().unwrap().raw_reply = None;
    }

    /// Accept and immediately close the connection.
    pub fn close_immediately(&self) {
        self.config.lock().unwrap().close_immediately = true;
    }

    /// Read the first command, then close without replying.
    pub fn read_then_close(&self) {
        self.config.lock().unwrap().read_then_close = true;
    }

    /// Reply to the first command then close the connection.
    pub fn close_after_first(&self) {
        self.config.lock().unwrap().close_after_first = true;
    }

    /// Read commands but never reply (client should time out).
    pub fn hang(&self) {
        self.config.lock().unwrap().hang = true;
    }

    /// Fragment responses into writes of `n` bytes with a short delay between
    /// them, exercising the client's chunked receive path.
    pub fn chunk(&self, n: usize) {
        self.config.lock().unwrap().chunk_bytes = n;
    }

    /// Every command received so far, in arrival order, across all
    /// connections (each entry is the full `{"type", "params"}` command).
    pub fn received(&self) -> Vec<Value> {
        self.received.lock().unwrap().clone()
    }

    /// The `params` of every command of the given type, in arrival order.
    pub fn params_of(&self, command_type: &str) -> Vec<Value> {
        self.received()
            .into_iter()
            .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some(command_type))
            .filter_map(|c| c.get("params").cloned())
            .collect()
    }

    /// Number of connections accepted so far.
    pub fn connection_count(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Stop the server thread. `MockBlender::drop` calls this too.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        // Unblock a pending accept, if any.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

impl Drop for MockBlender {
    fn drop(&mut self) {
        self.stop();
        // Do not join: the serve loop may be blocked reading from a client
        // that never disconnected (bounded by its 30s read timeout). The test
        // process exits soon anyway.
        self.thread.take();
    }
}

/// Serve one connection: read commands until the client disconnects and reply
/// to each, mirroring `BlenderMCPServer._handle_client`.
fn serve_connection(mut stream: TcpStream, config: &Mutex<MockConfig>, received: &Mutex<Vec<Value>>) {
    let (close_immediately, close_after_first, read_then_close, chunk_bytes, raw, hang, responses) = {
        let cfg = config.lock().unwrap();
        (
            cfg.close_immediately,
            cfg.close_after_first,
            cfg.read_then_close,
            cfg.chunk_bytes,
            cfg.raw_reply.clone(),
            cfg.hang,
            cfg.responses.clone(),
        )
    };
    if close_immediately {
        return; // drop(stream) closes the socket
    }
    if hang {
        // Drain without replying, keeping the socket open so the client's read
        // times out instead of seeing an EOF.
        let mut buf = [0u8; 8192];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                return;
            }
        }
        return;
    }

    let mut buffer: Vec<u8> = Vec::new();
    loop {
        let mut chunk = [0u8; 8192];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                // The client is strictly request/response on one socket, so a
                // complete JSON value means exactly one command is buffered.
                match serde_json::from_slice::<Value>(&buffer) {
                    Ok(command) => {
                        buffer.clear();
                        received.lock().unwrap().push(command.clone());
                        if read_then_close {
                            // Clean FIN: the server read the command, then
                            // hung up without replying.
                            return;
                        }
                        if let Some(raw) = &raw {
                            // Malformed response: write it, then close so the
                            // client sees a hard EOF instead of a long wait.
                            write_chunked(&mut stream, raw, chunk_bytes);
                            return;
                        }
                        let cmd_type = command
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default();
                        let response = responses
                            .get(cmd_type)
                            .cloned()
                            .unwrap_or_else(default_response)
                            .to_string()
                            .into_bytes();
                        write_chunked(&mut stream, &response, chunk_bytes);
                        if close_after_first {
                            return;
                        }
                    }
                    Err(_) => {
                        // Incomplete command; keep reading.
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(_) => break,
        }
    }
}

fn write_chunked(stream: &mut TcpStream, payload: &[u8], chunk_bytes: usize) {
    if chunk_bytes == 0 || chunk_bytes >= payload.len() {
        let _ = stream.write_all(payload);
        let _ = stream.flush();
        return;
    }
    for part in payload.chunks(chunk_bytes) {
        if stream.write_all(part).is_err() {
            return;
        }
        let _ = stream.flush();
        thread::sleep(Duration::from_millis(10));
    }
}
