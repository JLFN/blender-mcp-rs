//! PolyHaven integration tools. Ported 1:1 from `server.py`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::connection::BlenderConnection;
use crate::util::{get_bool, join_strings_array};
use rmcp::schemars;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetPolyHavenStatusRequest {
    pub user_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetPolyHavenCategoriesRequest {
    #[serde(default = "default_hdris")]
    pub asset_type: String,
    pub user_prompt: Option<String>,
}

fn default_hdris() -> String {
    "hdris".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct SearchPolyHavenAssetsRequest {
    #[serde(default = "default_all")]
    pub asset_type: String,
    pub categories: Option<String>,
    pub user_prompt: Option<String>,
}

fn default_all() -> String {
    "all".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct DownloadPolyHavenAssetRequest {
    pub asset_id: String,
    pub asset_type: String,
    #[serde(default = "default_1k")]
    pub resolution: String,
    pub file_format: Option<String>,
    pub user_prompt: Option<String>,
}

fn default_1k() -> String {
    "1k".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct SetTextureRequest {
    pub object_name: String,
    pub texture_id: String,
    pub user_prompt: Option<String>,
}

/// Port of `get_polyhaven_status`.
pub fn get_polyhaven_status(conn: &BlenderConnection, _req: &GetPolyHavenStatusRequest) -> String {
    match conn.send_command("get_polyhaven_status", None) {
        Ok(result) => {
            let enabled = get_bool(&result, "enabled", false);
            let mut message = result
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            if enabled {
                message.push_str(
                    "PolyHaven is good at Textures, and has a wider variety of textures than Sketchfab.",
                );
            }
            message
        }
        Err(e) => {
            tracing::error!(error = %e, "error checking PolyHaven status");
            format!("Error checking PolyHaven status: {e}")
        }
    }
}

/// Port of `get_polyhaven_categories`.
pub fn get_polyhaven_categories(conn: &BlenderConnection, req: &GetPolyHavenCategoriesRequest) -> String {

    let status = match conn.send_command("get_polyhaven_status", None) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "error getting Polyhaven categories");
            return format!("Error getting Polyhaven categories: {e}");
        }
    };
    if !get_bool(&status, "enabled", false) {
        return "PolyHaven integration is disabled. Select it in the sidebar in BlenderMCP, then run it again."
            .to_string();
    }

    let result = match conn.send_command(
        "get_polyhaven_categories",
        json!({ "asset_type": req.asset_type }).as_object(),
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "error getting Polyhaven categories");
            return format!("Error getting Polyhaven categories: {e}");
        }
    };

    if let Some(err) = result.get("error") {
        return format!("Error: {err}");
    }

    // categories: object of { category: count }
    let categories = result.get("categories").cloned().unwrap_or(Value::Null);
    let items: Vec<(String, i64)> = match categories.as_object() {
        Some(map) => map
            .iter()
            .map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(0)))
            .collect(),
        None => Vec::new(),
    };
    // Python `sorted(..., key=lambda x: x[1], reverse=True)` is a stable sort:
    // ties keep their insertion order.
    let mut idx: Vec<usize> = (0..items.len()).collect();
    idx.sort_by(|&a, &b| {
        items[b].1
            .cmp(&items[a].1)
            .then_with(|| a.cmp(&b))
    });
    let sorted: Vec<(String, i64)> = idx.iter().map(|&i| items[i].clone()).collect();

    let mut out = format!("Categories for {}:\n\n", req.asset_type);
    for (category, count) in sorted {
        out.push_str(&format!("- {category}: {count} assets\n"));
    }
    out
}

/// Port of `search_polyhaven_assets`.
pub fn search_polyhaven_assets(conn: &BlenderConnection, req: &SearchPolyHavenAssetsRequest) -> String {
    let result = match conn.send_command(
        "search_polyhaven_assets",
        json!({ "asset_type": req.asset_type, "categories": req.categories }).as_object(),
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "error searching Polyhaven assets");
            return format!("Error searching Polyhaven assets: {e}");
        }
    };

    if let Some(err) = result.get("error") {
        return format!("Error: {err}");
    }

    let assets = result.get("assets").cloned().unwrap_or(Value::Null);
    let total_count = result
        .get("total_count")
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    let returned_count = result
        .get("returned_count")
        .and_then(|x| x.as_i64())
        .unwrap_or(0);

    let mut out = format!("Found {total_count} assets");
    if let Some(categories) = req.categories.as_deref() {
        if !categories.is_empty() {
            out.push_str(&format!(" in categories: {categories}"));
        }
    }
    out.push_str(&format!("\nShowing {returned_count} assets:\n\n"));

    // assets: object of { asset_id: asset_data }
    let pairs: Vec<(String, Value)> = match assets.as_object() {
        Some(map) => map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        None => Vec::new(),
    };
    // Stable sort by download_count descending.
    let mut idx: Vec<usize> = (0..pairs.len()).collect();
    idx.sort_by(|&a, &b| {
        let da = pairs[a]
            .1
            .get("download_count")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        let db = pairs[b]
            .1
            .get("download_count")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        db.cmp(&da).then_with(|| a.cmp(&b))
    });

    const TYPE_NAMES: [&str; 3] = ["HDRI", "Texture", "Model"];

    for i in idx {
        let (asset_id, asset_data) = &pairs[i];
        let name = asset_data
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or(asset_id);
        out.push_str(&format!("- {name} (ID: {asset_id})\n"));

        let type_idx = asset_data.get("type").and_then(|x| x.as_i64()).unwrap_or(0);
        let type_name = TYPE_NAMES
            .get(type_idx as usize)
            .copied()
            .unwrap_or("Unknown");
        out.push_str(&format!("  Type: {type_name}\n"));

        let cat_list: Vec<String> = asset_data
            .get("categories")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        out.push_str(&format!("  Categories: {}\n", cat_list.join(", ")));

        let downloads = asset_data
            .get("download_count")
            .map(|x| x.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        out.push_str(&format!("  Downloads: {downloads}\n\n"));
    }
    out
}

/// Port of `download_polyhaven_asset`.
pub fn download_polyhaven_asset(conn: &BlenderConnection, req: &DownloadPolyHavenAssetRequest) -> String {
    let result = match conn.send_command(
        "download_polyhaven_asset",
        json!({
            "asset_id": req.asset_id,
            "asset_type": req.asset_type,
            "resolution": req.resolution,
            "file_format": req.file_format,
        })
        .as_object(),
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "error downloading Polyhaven asset");
            return format!("Error downloading Polyhaven asset: {e}");
        }
    };

    if let Some(err) = result.get("error") {
        return format!("Error: {err}");
    }

    if get_bool(&result, "success", false) {
        let message = result
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Asset downloaded and imported successfully")
            .to_string();

        if req.asset_type == "hdris" {
            format!("{message}. The HDRI has been set as the world environment.")
        } else if req.asset_type == "textures" {
            let material_name = result
                .get("material")
                .and_then(|m| m.as_str())
                .unwrap_or("");
            let maps = join_strings_array(&result, "maps");
            format!("{message}. Created material '{material_name}' with maps: {maps}.")
        } else if req.asset_type == "models" {
            format!("{message}. The model has been imported into the current scene.")
        } else {
            message
        }
    } else {
        let message = result
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown error");
        format!("Failed to download asset: {message}")
    }
}

/// Port of `set_texture`.
pub fn set_texture(conn: &BlenderConnection, req: &SetTextureRequest) -> String {
    let result = match conn.send_command(
        "set_texture",
        json!({ "object_name": req.object_name, "texture_id": req.texture_id }).as_object(),
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "error applying texture");
            return format!("Error applying texture: {e}");
        }
    };

    if let Some(err) = result.get("error") {
        return format!("Error: {err}");
    }

    if get_bool(&result, "success", false) {
        let material_name = result
            .get("material")
            .and_then(|m| m.as_str())
            .unwrap_or("");
        let maps = join_strings_array(&result, "maps");

        let material_info = result.get("material_info").cloned().unwrap_or(Value::Null);
        let node_count = material_info
            .get("node_count")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        let has_nodes = get_bool(&material_info, "has_nodes", false);
        let texture_nodes = material_info
            .get("texture_nodes")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();

        let mut output = format!(
            "Successfully applied texture '{}' to {}.\n",
            req.texture_id, req.object_name
        );
        output.push_str(&format!(
            "Using material '{material_name}' with maps: {maps}.\n\n"
        ));
        output.push_str(&format!("Material has nodes: {has_nodes}\n"));
        output.push_str(&format!("Total node count: {node_count}\n\n"));

        if texture_nodes.is_empty() {
            output.push_str("No texture nodes found in the material.\n");
        } else {
            output.push_str("Texture nodes:\n");
            for node in &texture_nodes {
                let name = node
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default();
                let image = node
                    .get("image")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default();
                output.push_str(&format!("- {name} using image: {image}\n"));
                let connections = node
                    .get("connections")
                    .and_then(|x| x.as_array())
                    .cloned()
                    .unwrap_or_default();
                if !connections.is_empty() {
                    output.push_str("  Connections:\n");
                    for conn in &connections {
                        // Python prints the plain string; `Value::to_string`
                        // would add JSON quote marks, so use `as_str`.
                        output.push_str(&format!(
                            "    {}\n",
                            conn.as_str().unwrap_or_default()
                        ));
                    }
                }
            }
        }
        output
    } else {
        let message = result
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown error");
        format!("Failed to apply texture: {message}")
    }
}