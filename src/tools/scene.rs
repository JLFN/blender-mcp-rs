//! Core scene tools: scene info, object info, viewport screenshot, and raw
//! code execution. Ported 1:1 from `server.py`.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::connection::BlenderConnection;
use crate::util::{get_str, to_json_pretty};
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars;
use rmcp::ErrorData;

/// Request for the get_scene_info tool: reports the current Blender scene.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetSceneInfoRequest {
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Request for the get_object_info tool: queries one named object's details.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetObjectInfoRequest {
    /// Name of the object to query.
    pub object_name: String,
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Request for the get_viewport_screenshot tool: captures the 3D viewport.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetViewportScreenshotRequest {
    /// Largest dimension of the captured screenshot in pixels.
    #[serde(default = "default_max_size")]
    pub max_size: u32,
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

fn default_max_size() -> u32 {
    1000
}

/// Request for the execute_blender_code tool: runs raw Python inside Blender.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct ExecuteBlenderCodeRequest {
    /// Python code to execute inside Blender.
    pub code: String,
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Port of `get_scene_info`: returns the JSON the addon produced.
pub fn get_scene_info(conn: &BlenderConnection, _req: &GetSceneInfoRequest) -> String {
    match conn.send_command("get_scene_info", None) {
        Ok(result) => to_json_pretty(&result),
        Err(e) => {
            tracing::error!(error = %e, "error getting scene info from Blender");
            format!("Error getting scene info: {e}")
        }
    }
}

/// Port of `get_object_info`: returns compact scene object JSON.
pub fn get_object_info(conn: &BlenderConnection, req: &GetObjectInfoRequest) -> String {
    let params = json!({ "name": req.object_name });
    match conn.send_command("get_object_info", params.as_object()) {
        Ok(result) => to_json_pretty(&result),
        Err(e) => {
            tracing::error!(error = %e, "error getting object info from Blender");
            format!("Error getting object info: {e}")
        }
    }
}

/// Port of `get_viewport_screenshot`: asks Blender to render a PNG to a temp
/// file, reads it back, removes it, and returns the bytes as an image content
/// block.
pub fn get_viewport_screenshot(conn: &BlenderConnection, req: &GetViewportScreenshotRequest) -> Result<CallToolResult, ErrorData> {

    let temp_path = std::env::temp_dir().join(format!(
        "blender_screenshot_{}.png",
        std::process::id()
    ));

    let params = json!({
        "max_size": req.max_size,
        "filepath": temp_path.to_string_lossy(),
        "format": "png"
    });

    let result = conn
        .send_command("get_viewport_screenshot", params.as_object())
        .map_err(|e| ErrorData::internal_error(format!("Screenshot failed: {e}"), None))?;

    if let Some(err) = result.get("error") {
        return Err(ErrorData::internal_error(
            format!("Screenshot failed: {err}"),
            None,
        ));
    }

    if !temp_path.exists() {
        return Err(ErrorData::internal_error(
            "Screenshot file was not created",
            None,
        ));
    }

    let image_bytes = std::fs::read(&temp_path).map_err(|e| {
        ErrorData::internal_error(format!("Screenshot failed: {e}"), None)
    })?;

    let _ = std::fs::remove_file(&temp_path);

    let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
    Ok(CallToolResult::success(vec![ContentBlock::image(
        b64,
        "image/png",
    )]))
}

/// Port of `execute_blender_code`: forwards raw Python to be `exec`'d inside
/// Blender and returns the captured stdout.
pub fn execute_blender_code(conn: &BlenderConnection, req: &ExecuteBlenderCodeRequest) -> String {
    let params = json!({ "code": req.code });
    match conn.send_command("execute_code", params.as_object()) {
        Ok(result) => {
            let output = get_str(&result, "result");
            format!("Code executed successfully: {output}")
        }
        Err(e) => {
            tracing::error!(error = %e, "error executing code");
            format!("Error executing code: {e}")
        }
    }
}

/// Parse `Value` back into its pretty form for callers wanting the raw JSON.
#[allow(dead_code)]
pub fn debug_value(v: &Value) -> String {
    to_json_pretty(v)
}