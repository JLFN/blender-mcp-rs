//! Hyper3D (Rodin) generation tools. Ported 1:1 from `server.py`.
//!
//! One intentional fix: the Python source validates the image *URL* branch
//! against `input_image_paths` (a variable that is necessarily `None` there),
//! which makes that branch always error. Here the URL branch validates the
//! URLs, which is the obviously intended behavior.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bbox::process_bbox;
use crate::connection::BlenderConnection;
use crate::util::{is_truthy, to_json_pretty};
use rmcp::schemars;

/// Request for the get_hyper3d_status tool: checks the Hyper3D Rodin addon state.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetHyper3dStatusRequest {
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Request for the generate_hyper3d_model_via_text tool: generates a model from a description.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GenerateHyper3dModelViaTextRequest {
    /// Short English description of the desired model.
    pub text_prompt: String,
    /// Optional Length/Width/Height ratio of the generated model.
    pub bbox_condition: Option<Vec<f64>>,
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Request for the generate_hyper3d_model_via_images tool: generates a model from reference images.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GenerateHyper3dModelViaImagesRequest {
    /// Absolute paths of the input images; required in MAIN_SITE mode.
    pub input_image_paths: Option<Vec<String>>,
    /// URLs of the input images; required in FAL_AI mode.
    pub input_image_urls: Option<Vec<String>>,
    /// Optional Length/Width/Height ratio of the generated model.
    pub bbox_condition: Option<Vec<f64>>,
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Request for the poll_rodin_job_status tool: polls a Rodin generation job.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct PollRodinJobStatusRequest {
    /// Subscription key from the generate step; used in MAIN_SITE mode.
    pub subscription_key: Option<String>,
    /// Request ID from the generate step; used in FAL_AI mode.
    pub request_id: Option<String>,
}

/// Request for the import_generated_asset tool: imports a finished Rodin model.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct ImportGeneratedAssetRequest {
    /// Name of the object in the scene.
    pub name: String,
    /// Task UUID from the generate step; used in MAIN_SITE mode.
    pub task_uuid: Option<String>,
    /// Request ID from the generate step; used in FAL_AI mode.
    pub request_id: Option<String>,
}

/// Normalize an optional raw bbox array through `_process_bbox`'s logic.
fn normalized_bbox(bbox: Option<&Vec<f64>>) -> Result<Option<Value>, String> {
    let raw = bbox.map(|v| json!(v));
    process_bbox(raw.as_ref())
}

/// Port of `get_hyper3d_status`.
pub fn get_hyper3d_status(conn: &BlenderConnection, _req: &GetHyper3dStatusRequest) -> String {
    match conn.send_command("get_hyper3d_status", None) {
        Ok(result) => result
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string(),
        Err(e) => {
            tracing::error!(error = %e, "error checking Hyper3D status");
            format!("Error checking Hyper3D status: {e}")
        }
    }
}

/// Format the `create_rodin_job` response the same way for both generation
/// entry points.
fn format_rodin_submission(result: &Value) -> String {
    let succeed = is_truthy(result.get("submit_time"));
    if succeed {
        // `Value::to_string()` serializes as JSON (a string gains quote marks);
        // the Python original reads plain string fields, so extract them with
        // `as_str()` to avoid double-quoted values in the re-serialized output.
        let task_uuid = result
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let subscription_key = result
            .get("jobs")
            .and_then(|j| j.get("subscription_key"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        to_json_pretty(&json!({
            "task_uuid": task_uuid,
            "subscription_key": subscription_key,
        }))
    } else {
        to_json_pretty(result)
    }
}

/// Port of `generate_hyper3d_model_via_text`.
pub fn generate_hyper3d_model_via_text(conn: &BlenderConnection, req: &GenerateHyper3dModelViaTextRequest) -> String {
    let bbox = match normalized_bbox(req.bbox_condition.as_ref()) {
        Ok(b) => b,
        Err(e) => return format!("Error generating Hyper3D task: {e}"),
    };
    let params = json!({
        "text_prompt": req.text_prompt,
        "images": Value::Null,
        "bbox_condition": bbox,
    });
    match conn.send_command("create_rodin_job", params.as_object()) {
        Ok(result) => format_rodin_submission(&result),
        Err(e) => {
            tracing::error!(error = %e, "error generating Hyper3D task");
            format!("Error generating Hyper3D task: {e}")
        }
    }
}

/// Port of `generate_hyper3d_model_via_images`.
pub fn generate_hyper3d_model_via_images(conn: &BlenderConnection, req: &GenerateHyper3dModelViaImagesRequest) -> String {
    if req.input_image_paths.is_some() && req.input_image_urls.is_some() {
        return "Error: Conflict parameters given!".to_string();
    }
    if req.input_image_paths.is_none() && req.input_image_urls.is_none() {
        return "Error: No image given!".to_string();
    }

    let images: Value = if let Some(paths) = &req.input_image_paths {
        if !paths.iter().all(|p| std::path::Path::new(p).exists()) {
            return "Error: not all image paths are valid!".to_string();
        }
        let mut encoded: Vec<Value> = Vec::new();
        for path in paths {
            match std::fs::read(path) {
                Ok(bytes) => {
                    let suffix = std::path::Path::new(path)
                        .extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    encoded.push(json!([suffix, b64]));
                }
                Err(e) => {
                    tracing::error!(path = %path, error = %e, "failed to read image");
                    return format!("Error generating Hyper3D task: {e}");
                }
            }
        }
        Value::Array(encoded)
    } else {
        let urls = req.input_image_urls.clone().unwrap_or_default();
        // Python's intent: every URL must parse. urlparse() always yields a
        // truthy ParseResult for any string, so mirror the weak check as a
        // parseability check (non-empty input).
        if !urls.iter().all(|u| !u.trim().is_empty()) {
            return "Error: not all image URLs are valid!".to_string();
        }
        Value::Array(urls.into_iter().map(Value::String).collect())
    };

    let bbox = match normalized_bbox(req.bbox_condition.as_ref()) {
        Ok(b) => b,
        Err(e) => return format!("Error generating Hyper3D task: {e}"),
    };
    let params = json!({
        "text_prompt": Value::Null,
        "images": images,
        "bbox_condition": bbox,
    });
    match conn.send_command("create_rodin_job", params.as_object()) {
        Ok(result) => format_rodin_submission(&result),
        Err(e) => {
            tracing::error!(error = %e, "error generating Hyper3D task");
            format!("Error generating Hyper3D task: {e}")
        }
    }
}

/// Port of `poll_rodin_job_status`.
pub fn poll_rodin_job_status(conn: &BlenderConnection, req: &PollRodinJobStatusRequest) -> String {
    let mut params = serde_json::Map::new();
    if let Some(sub) = &req.subscription_key {
        if !sub.is_empty() {
            params.insert("subscription_key".to_string(), Value::String(sub.clone()));
        }
    } else if let Some(rid) = &req.request_id {
        if !rid.is_empty() {
            params.insert("request_id".to_string(), Value::String(rid.clone()));
        }
    }
    match conn.send_command("poll_rodin_job_status", Some(&params)) {
        Ok(result) => to_json_pretty(&result),
        Err(e) => {
            tracing::error!(error = %e, "error generating Hyper3D task");
            format!("Error generating Hyper3D task: {e}")
        }
    }
}

/// Port of `import_generated_asset`.
pub fn import_generated_asset(conn: &BlenderConnection, req: &ImportGeneratedAssetRequest) -> String {
    let mut params = serde_json::Map::new();
    params.insert("name".to_string(), Value::String(req.name.clone()));
    if let Some(uuid) = &req.task_uuid {
        if !uuid.is_empty() {
            params.insert("task_uuid".to_string(), Value::String(uuid.clone()));
        }
    } else if let Some(rid) = &req.request_id {
        if !rid.is_empty() {
            params.insert("request_id".to_string(), Value::String(rid.clone()));
        }
    }
    match conn.send_command("import_generated_asset", Some(&params)) {
        Ok(result) => to_json_pretty(&result),
        Err(e) => {
            tracing::error!(error = %e, "error generating Hyper3D task");
            format!("Error generating Hyper3D task: {e}")
        }
    }
}