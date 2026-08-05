//! Sketchfab integration tools. Ported 1:1 from `server.py`.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::connection::BlenderConnection;
use crate::util::get_bool;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars;
use rmcp::ErrorData;

/// Request for the get_sketchfab_status tool: checks the Sketchfab addon state.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetSketchfabStatusRequest {
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Request for the search_sketchfab_models tool: searches Sketchfab with optional filters.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct SearchSketchfabModelsRequest {
    /// Text to search for.
    pub query: String,
    /// Optional comma-separated list of categories.
    pub categories: Option<String>,
    /// Maximum number of results to return.
    #[serde(default = "default_count")]
    pub count: i64,
    /// Whether to include only downloadable models.
    #[serde(default = "default_true")]
    pub downloadable: bool,
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

fn default_count() -> i64 {
    20
}

fn default_true() -> bool {
    true
}

/// Request for the get_sketchfab_model_preview tool: fetches a model thumbnail.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetSketchfabModelPreviewRequest {
    /// Unique identifier of the Sketchfab model to preview.
    pub uid: String,
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Request for the download_sketchfab_model tool: downloads and imports a model.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct DownloadSketchfabModelRequest {
    /// Unique identifier of the Sketchfab model to download.
    pub uid: String,
    /// Target size in Blender units for the largest dimension of the model.
    pub target_size: f64,
    /// The original user prompt that led to this tool call; never forwarded to the addon.
    pub user_prompt: Option<String>,
}

/// Port of `get_sketchfab_status`.
pub fn get_sketchfab_status(conn: &BlenderConnection, _req: &GetSketchfabStatusRequest) -> String {
    match conn.send_command("get_sketchfab_status", None) {
        Ok(result) => {
            let enabled = get_bool(&result, "enabled", false);
            let mut message = result
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            if enabled {
                message.push_str(
                    "Sketchfab is good at Realistic models, and has a wider variety of models than PolyHaven.",
                );
            }
            message
        }
        Err(e) => {
            tracing::error!(error = %e, "error checking Sketchfab status");
            format!("Error checking Sketchfab status: {e}")
        }
    }
}

/// Port of `search_sketchfab_models`.
pub fn search_sketchfab_models(conn: &BlenderConnection, req: &SearchSketchfabModelsRequest) -> String {
    tracing::info!(
        query = %req.query,
        categories = ?req.categories,
        count = req.count,
        downloadable = req.downloadable,
        "searching Sketchfab models"
    );
    let result = match conn.send_command(
        "search_sketchfab_models",
        json!({
            "query": req.query,
            "categories": req.categories,
            "count": req.count,
            "downloadable": req.downloadable,
        })
        .as_object(),
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "error searching Sketchfab models");
            return format!("Error searching Sketchfab models: {e}");
        }
    };

    if let Some(err) = result.get("error") {
        tracing::error!(error = %err, "error from Sketchfab search");
        return format!("Error: {err}");
    }

    let models = result
        .get("results")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    if models.is_empty() {
        return format!("No models found matching '{}'", req.query);
    }

    let mut out = format!("Found {} models matching '{}':\n\n", models.len(), req.query);
    for model in &models {
        if model.is_null() {
            continue;
        }
        let model_name = model
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("Unnamed model");
        let model_uid = model
            .get("uid")
            .and_then(|x| x.as_str())
            .unwrap_or("Unknown ID");
        out.push_str(&format!("- {model_name} (UID: {model_uid})\n"));

        // user dict with safety checks
        let username = model
            .get("user")
            .and_then(|u| u.as_object())
            .and_then(|u| u.get("username"))
            .and_then(|x| x.as_str())
            .unwrap_or("Unknown author");
        out.push_str(&format!("  Author: {username}\n"));

        // license dict with safety checks
        let license_label = model
            .get("license")
            .and_then(|l| l.as_object())
            .and_then(|l| l.get("label"))
            .and_then(|x| x.as_str())
            .unwrap_or("Unknown");
        out.push_str(&format!("  License: {license_label}\n"));

        let face_count = model
            .get("faceCount")
            .map(|x| x.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        let is_downloadable = if get_bool(model, "isDownloadable", false) {
            "Yes"
        } else {
            "No"
        };
        out.push_str(&format!("  Face count: {face_count}\n"));
        out.push_str(&format!("  Downloadable: {is_downloadable}\n\n"));
    }
    out
}

/// Port of `get_sketchfab_model_preview`: returns the thumbnail as an image
/// content block. Errors are raised as MCP tool errors, exactly like the
/// Python `raise Exception`.
pub fn get_sketchfab_model_preview(
    conn: &BlenderConnection,
    req: &GetSketchfabModelPreviewRequest,
) -> Result<CallToolResult, ErrorData> {
    tracing::info!(uid = %req.uid, "getting Sketchfab model preview");
    let result = conn
        .send_command("get_sketchfab_model_preview", json!({ "uid": req.uid }).as_object())
        .map_err(|e| ErrorData::internal_error(format!("Failed to get preview: {e}"), None))?;

    if result.is_null() {
        return Err(ErrorData::internal_error(
            "Failed to get preview: Received no response from Blender",
            None,
        ));
    }
    if let Some(err) = result.get("error") {
        return Err(ErrorData::internal_error(
            format!("Failed to get preview: {err}"),
            None,
        ));
    }

    let image_b64 = result
        .get("image_data")
        .and_then(|x| x.as_str())
        .ok_or_else(|| {
            ErrorData::internal_error("Failed to get preview: missing image_data", None)
        })?;
    // Validate the base64 payload (Python decodes it before handing it to the
    // MCP library, which re-encodes it for transport).
    base64::engine::general_purpose::STANDARD
        .decode(image_b64)
        .map_err(|e| {
            ErrorData::internal_error(format!("Failed to get preview: invalid image data: {e}"), None)
        })?;

    let img_format = result
        .get("format")
        .and_then(|x| x.as_str())
        .unwrap_or("jpeg")
        .to_string();

    let model_name = result
        .get("model_name")
        .and_then(|x| x.as_str())
        .unwrap_or("Unknown");
    let author = result
        .get("author")
        .and_then(|x| x.as_str())
        .unwrap_or("Unknown");
    tracing::info!(model_name, author, "preview retrieved");

    let mime = match img_format.as_str() {
        "png" => "image/png".to_string(),
        "webp" => "image/webp".to_string(),
        _ => "image/jpeg".to_string(),
    };
    Ok(CallToolResult::success(vec![ContentBlock::image(
        image_b64.to_string(),
        mime,
    )]))
}

/// Port of `download_sketchfab_model`.
pub fn download_sketchfab_model(conn: &BlenderConnection, req: &DownloadSketchfabModelRequest) -> String {
    tracing::info!(uid = %req.uid, target_size = req.target_size, "downloading Sketchfab model");
    let result = match conn.send_command(
        "download_sketchfab_model",
        json!({
            "uid": req.uid,
            "normalize_size": true,
            "target_size": req.target_size,
        })
        .as_object(),
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "error downloading Sketchfab model");
            return format!("Error downloading Sketchfab model: {e}");
        }
    };

    if result.is_null() {
        tracing::error!("received None result from Sketchfab download");
        return "Error: Received no response from Sketchfab download request".to_string();
    }
    if let Some(err) = result.get("error") {
        tracing::error!(error = %err, "error from Sketchfab download");
        return format!("Error: {err}");
    }

    if get_bool(&result, "success", false) {
        let imported_objects = result
            .get("imported_objects")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        let object_names = if imported_objects.is_empty() {
            "none".to_string()
        } else {
            imported_objects
                .iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };

        let mut output = "Successfully imported model.\n".to_string();
        output.push_str(&format!("Created objects: {object_names}\n"));

        if let Some(dims) = result.get("dimensions").and_then(|d| d.as_array()) {
            if dims.len() >= 3 {
                let x = dims[0].as_f64().unwrap_or(0.0);
                let y = dims[1].as_f64().unwrap_or(0.0);
                let z = dims[2].as_f64().unwrap_or(0.0);
                output.push_str(&format!(
                    "Dimensions (X, Y, Z): {x:.3} x {y:.3} x {z:.3} meters\n"
                ));
            }
        }

        if let Some(bbox) = result.get("world_bounding_box").and_then(|b| b.as_array()) {
            if bbox.len() >= 2 {
                output.push_str(&format!(
                    "Bounding box: min={}, max={}\n",
                    bbox[0], bbox[1]
                ));
            }
        }

        if get_bool(&result, "normalized", false) {
            let scale = result
                .get("scale_applied")
                .and_then(|s| s.as_f64())
                .unwrap_or(1.0);
            output.push_str(&format!(
                "Size normalized: scale factor {scale:.6} applied (target size: {}m)\n",
                req.target_size
            ));
        }

        output
    } else {
        let message = result
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown error");
        format!("Failed to download model: {message}")
    }
}