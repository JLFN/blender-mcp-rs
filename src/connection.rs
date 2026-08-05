//! Rust port of the `BlenderConnection` class from the original Python
//! `src/blender_mcp/server.py`.
//!
//! The addon (addon/addon.py, which runs inside Blender and drives `bpy`) exposes
//! a plain TCP JSON socket server. This module is the client half of that wire
//! protocol: it connects, sends a JSON command of the form
//! `{"type": <command>, "params": {...}}`, and reads back every byte until a
//! complete JSON object has been received, mirroring the Python
//! `receive_full_response` framing and its 180 second timeout.
//!
//! As in the Python original, the socket that serves the addon has no
//! request/response correlation: the response is matched to the command purely
//! by ordering on the stream. `send_command` therefore holds a single Mutex
//! across send + receive so two commands can never interleave and hand each
//! other's responses back.

use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{Map, Value};

/// Default connection parameters, matching the Python module's constants.
pub const DEFAULT_HOST: &str = "localhost";
pub const DEFAULT_PORT: u16 = 9876;

/// How long to wait for a complete response from the addon. Matches the addon's
/// timeout and the Python client's 180.0 second `settimeout`.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(180);

/// Read chunk size used while assembling a complete JSON response.
const BUFFER_SIZE: usize = 8192;

/// Error type returned by the Blender connection layer.
#[derive(Debug, Clone)]
pub struct BlenderError {
    pub message: String,
}

impl BlenderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BlenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BlenderError {}

impl From<std::io::Error> for BlenderError {
    fn from(e: std::io::Error) -> Self {
        BlenderError::new(e.to_string())
    }
}

/// Holds the (mutable) socket state behind the connection lock.
struct BlenderInner {
    sock: Option<TcpStream>,
}

impl BlenderInner {
    fn new() -> Self {
        Self { sock: None }
    }
}

/// A persistent, thread-safe client for the Blender addon socket server.
///
/// Like the Python `get_blender_connection`, a dead socket is *not* probed
/// before reuse: the next real command detects it and reconnects then. This
/// avoids putting two commands on the wire per tool call.
pub struct BlenderConnection {
    host: String,
    port: u16,
    timeout: Duration,
    inner: Mutex<BlenderInner>,
}

impl BlenderConnection {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            timeout: RESPONSE_TIMEOUT,
            inner: Mutex::new(BlenderInner::new()),
        }
    }

    /// Override the response read timeout (default 180s). Used by tests to
    /// make a dead-yet-accepting server fail quickly.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Build a connection from the `BLENDER_HOST` / `BLENDER_PORT` environment
    /// variables, falling back to `localhost:9876`.
    pub fn from_env() -> Self {
        let host = std::env::var("BLENDER_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
        let port = std::env::var("BLENDER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        Self::new(host, port)
    }

    fn connect(&self, inner: &mut BlenderInner) -> bool {
        if inner.sock.is_some() {
            return true;
        }
        let addr = (self.host.as_str(), self.port);
        match addr.to_socket_addrs() {
            Ok(mut addrs) => {
                if let Some(resolved) = addrs.next() {
                    match TcpStream::connect(resolved) {
                        Ok(stream) => {
                            let _ = stream.set_read_timeout(Some(RESPONSE_TIMEOUT));
                            tracing::info!(
                                host = %self.host,
                                port = self.port,
                                "connected to Blender"
                            );
                            inner.sock = Some(stream);
                            true
                        }
                        Err(e) => {
                            tracing::error!(
                                host = %self.host,
                                port = self.port,
                                error = %e,
                                "failed to connect to Blender"
                            );
                            inner.sock = None;
                            false
                        }
                    }
                } else {
                    tracing::error!(host = %self.host, port = self.port, "could not resolve Blender host");
                    false
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "could not resolve Blender host");
                false
            }
        }
    }

    /// Close the socket, if any, and reset the connection to a fresh state.
    pub fn disconnect(&self) {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(sock) = inner.sock.take() {
            let _ = sock.shutdown(std::net::Shutdown::Both);
        }
    }

    /// Send one command to Blender and return the JSON `result`.
    ///
    /// Holds the lock across send + receive so the response pairing by ordering
    /// on the stream can never be corrupted by a concurrent call.
    pub fn send_command(&self, command_type: &str, params: Option<&Map<String, Value>>) -> Result<Value, BlenderError> {
        let mut inner = self.inner.lock().map_err(|_| BlenderError::new("connection lock poisoned"))?;
        self.send_command_locked(&mut inner, command_type, params)
    }

    fn send_command_locked(
        &self,
        inner: &mut BlenderInner,
        command_type: &str,
        params: Option<&Map<String, Value>>,
    ) -> Result<Value, BlenderError> {
        if inner.sock.is_none() && !self.connect(inner) {
            return Err(BlenderError::new("Not connected to Blender"));
        }

        let command = serde_json::json!({
            "type": command_type,
            "params": params.cloned().unwrap_or_default(),
        });
        let payload = command.to_string();

        // Take the socket out so `inner` is free to mutate while we hold the
        // stream; a failed send drops it (invalidating the connection so the
        // next call reconnects), a successful send puts it back.
        let mut sock = inner
            .sock
            .take()
            .ok_or_else(|| BlenderError::new("Not connected to Blender"))?;

        sock.set_read_timeout(Some(self.timeout)).map_err(|e| {
            inner.sock = None;
            BlenderError::new(format!("failed to set socket timeout: {e}"))
        })?;

        tracing::info!(command_type, %command, "sending command");

        // Send the full command. On a broken pipe / reset, the socket is
        // dropped (never put back), matching the Python invalidation so the
        // connection reconnects on the next real command.
        if let Err(e) = sock.write_all(payload.as_bytes()) {
            let message = match e.kind() {
                ErrorKind::BrokenPipe | ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => {
                    format!("Connection to Blender lost: {e}")
                }
                _ => format!("Error communicating with Blender: {e}"),
            };
            tracing::error!(error = %message, "failed to send command");
            return Err(BlenderError::new(message));
        }

        inner.sock = Some(sock);

        let response_data = self.receive_full_response(inner)?;
        let response: Value = serde_json::from_slice(&response_data).map_err(|e| {
            tracing::error!(error = %e, "invalid JSON response from Blender");
            inner.sock = None;
            BlenderError::new(format!(
                "Invalid response from Blender: {e}\nRaw (first 200 bytes): {}",
                String::from_utf8_lossy(&response_data[..response_data.len().min(200)])
            ))
        })?;

        if response.get("status").and_then(|s| s.as_str()) == Some("error") {
            let message = response
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error from Blender")
                .to_string();
            tracing::error!(message = %message, "Blender error");
            return Err(BlenderError::new(message));
        }

        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Receive the complete JSON response, potentially in multiple chunks.
    ///
    /// Mirrors the Python `receive_full_response`: bytes accumulate until the
    /// buffer parses as a complete JSON value. On a socket timeout, whatever
    /// was received so far is returned if it parses, otherwise an error is
    /// raised. The socket is taken from `inner` and only restored on success;
    /// any failure drops it so the next real command reconnects.
    fn receive_full_response(&self, inner: &mut BlenderInner) -> Result<Vec<u8>, BlenderError> {
        let mut sock = match inner.sock.take() {
            Some(s) => s,
            None => return Err(BlenderError::new("Not connected to Blender")),
        };
        let mut chunks: Vec<u8> = Vec::new();
        let mut errored = false;

        loop {
            let mut chunk = [0u8; BUFFER_SIZE];
            match sock.read(&mut chunk) {
                Ok(0) => {
                    // Empty chunk: either a clean break (we have data) or a
                    // closed-before-any-data error.
                    if chunks.is_empty() {
                        return Err(BlenderError::new(
                            "Connection closed before receiving any data",
                        ));
                    }
                    break;
                }
                Ok(n) => {
                    chunks.extend_from_slice(&chunk[..n]);
                    if serde_json::from_slice::<Value>(&chunks).is_ok() {
                        tracing::info!(bytes = chunks.len(), "received complete response");
                        inner.sock = Some(sock);
                        return Ok(chunks);
                    }
                    // Incomplete JSON, keep reading.
                }
                Err(e) => match e.kind() {
                    ErrorKind::WouldBlock | ErrorKind::TimedOut => {
                        tracing::warn!("socket timeout during chunked receive");
                        break;
                    }
                    ErrorKind::BrokenPipe
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted => {
                        errored = true;
                        tracing::error!(error = %e, "socket connection error during receive");
                        break;
                    }
                    other => {
                        errored = true;
                        tracing::error!(error = %other, "error during receive");
                        break;
                    }
                },
            }
        }

        // From here on `sock` is dropped (inner stays None) on failure, so the
        // next command reconnects.
        if errored {
            return Err(BlenderError::new("Socket connection error during receive"));
        }

        if chunks.is_empty() {
            return Err(BlenderError::new("No data received"));
        }

        let data = chunks;
        match serde_json::from_slice::<Value>(&data) {
            Ok(_) => {
                tracing::info!(bytes = data.len(), "returning data after receive completion");
                inner.sock = Some(sock);
                Ok(data)
            }
            Err(e) => Err(BlenderError::new(format!(
                "Incomplete JSON response received: {e}"
            ))),
        }
    }
}

/// Global persistent Blender connection, mirroring the module-level
/// `_blender_connection` used by resources and tools.
static BLENDER_CONNECTION: OnceLock<Arc<BlenderConnection>> = OnceLock::new();

/// Get or create the process-wide Blender connection.
pub fn get_blender_connection() -> Arc<BlenderConnection> {
    BLENDER_CONNECTION
        .get_or_init(|| Arc::new(BlenderConnection::from_env()))
        .clone()
}