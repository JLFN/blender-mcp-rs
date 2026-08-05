//! Hunyuan3D integration tools. Ported 1:1 from `server.py`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::connection::BlenderConnection;
use crate::util::{is_truthy, to_json_pretty};
use rmcp::schemars;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GetHunyuan3dStatusRequest {
    pub user_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct GenerateHunyuan3dModelRequest {
    pub text_prompt: Option<String>,
    pub input_image_url: Option<String>,
    pub user_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct PollHunyuanJobStatusRequest {
    pub job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct ImportGeneratedAssetHunyuanRequest {
    pub name: String,
    pub zip_file_url: Option<String>,
}

/// Port of `get_hunyuan3d_status`.
pub fn get_hunyuan3d_status(conn: &BlenderConnection, _req: &GetHunyuan3dStatusRequest) -> String {
    match conn.send_command("get_hunyuan3d_status", None) {
        Ok(result) => result
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string(),
        Err(e) => {
            tracing::error!(error = %e, "error checking Hunyuan3D status");
            format!("Error checking Hunyuan3D status: {e}")
        }
    }
}

/// Port of `generate_hunyuan3d_model`.
pub fn generate_hunyuan3d_model(conn: &BlenderConnection, req: &GenerateHunyuan3dModelRequest) -> String {
    let params = json!({
        "text_prompt": req.text_prompt,
        "image": req.input_image_url,
    });
    match conn.send_command("create_hunyuan_job", params.as_object()) {
        Ok(result) => {
            let job_id = result
                .get("Response")
                .and_then(|r| r.get("JobId"))
                .and_then(|j| j.as_str());
            match job_id {
                Some(jid) => {
                    let formatted_job_id = format!("job_{jid}");
                    to_json_pretty(&json!({ "job_id": formatted_job_id }))
                }
                None => to_json_pretty(&result),
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "error generating Hunyuan3D task");
            format!("Error generating Hunyuan3D task: {e}")
        }
    }
}

/// Port of `poll_hunyuan_job_status`.
pub fn poll_hunyuan_job_status(conn: &BlenderConnection, req: &PollHunyuanJobStatusRequest) -> String {
    let params = json!({ "job_id": req.job_id });
    match conn.send_command("poll_hunyuan_job_status", params.as_object()) {
        Ok(result) => to_json_pretty(&result),
        Err(e) => {
            tracing::error!(error = %e, "error generating Hunyuan3D task");
            format!("Error generating Hunyuan3D task: {e}")
        }
    }
}

/// Port of `import_generated_asset_hunyuan`.
pub fn import_generated_asset_hunyuan(conn: &BlenderConnection, req: &ImportGeneratedAssetHunyuanRequest) -> String {
    let mut params = serde_json::Map::new();
    params.insert("name".to_string(), Value::String(req.name.clone()));
    if let Some(url) = &req.zip_file_url {
        if !url.is_empty() {
            params.insert("zip_file_url".to_string(), Value::String(url.clone()));
        }
    }
    match conn.send_command("import_generated_asset_hunyuan", Some(&params)) {
        Ok(result) => to_json_pretty(&result),
        Err(e) => {
            tracing::error!(error = %e, "error generating Hunyuan3D task");
            format!("Error generating Hunyuan3D task: {e}")
        }
    }
}

/// Port of the `asset_creation_strategy` prompt (1:1 text).
pub fn asset_creation_strategy() -> String {
    let _ = is_truthy; // keep util import used even if refactored
    ASSET_STRATEGY_PROMPT.to_string()
}

/// Verbatim text of the Python `asset_creation_strategy` prompt.
const ASSET_STRATEGY_PROMPT: &str = r#"When creating 3D content in Blender, always start by checking if integrations are available:

    0. Before anything, always check the scene from get_scene_info()
    
    **IMPORTANT: Visual Verification**
    - Use get_viewport_screenshot() BEFORE making changes to see the current state
    - Use get_viewport_screenshot() AFTER executing code or importing assets to verify the result
    - This helps confirm your changes worked as expected and catch any visual issues
    1. First use the following tools to verify if the following integrations are enabled:
        1. PolyHaven
            Use get_polyhaven_status() to verify its status
            If PolyHaven is enabled:
            - For objects/models: Use download_polyhaven_asset() with asset_type="models"
            - For materials/textures: Use download_polyhaven_asset() with asset_type="textures"
            - For environment lighting: Use download_polyhaven_asset() with asset_type="hdris"
        2. Sketchfab
            Sketchfab is good at Realistic models, and has a wider variety of models than PolyHaven.
            Use get_sketchfab_status() to verify its status
            If Sketchfab is enabled:
            - For objects/models: First search using search_sketchfab_models() with your query
            - Then download specific models using download_sketchfab_model() with the UID
            - Note that only downloadable models can be accessed, and API key must be properly configured
            - Sketchfab has a wider variety of models than PolyHaven, especially for specific subjects
        3. Hyper3D(Rodin)
            Hyper3D Rodin is good at generating 3D models for single item.
            So don't try to:
            1. Generate the whole scene with one shot
            2. Generate ground using Hyper3D
            3. Generate parts of the items separately and put them together afterwards

            Use get_hyper3d_status() to verify its status
            If Hyper3D is enabled:
            - For objects/models, do the following steps:
                1. Create the model generation task
                    - Use generate_hyper3d_model_via_images() if image(s) is/are given
                    - Use generate_hyper3d_model_via_text() if generating 3D asset using text prompt
                    If key type is free_trial and insufficient balance error returned, tell the user that the free trial key can only generated limited models everyday, they can choose to:
                    - Wait for another day and try again
                    - Go to hyper3d.ai to find out how to get their own API key
                    - Go to fal.ai to get their own private API key
                2. Poll the status
                    - Use poll_rodin_job_status() to check if the generation task has completed or failed
                3. Import the asset
                    - Use import_generated_asset() to import the generated GLB model the asset
                4. After importing the asset, ALWAYS check the world_bounding_box of the imported mesh, and adjust the mesh's location and size
                    Adjust the imported mesh's location, scale, rotation, so that the mesh is on the right spot.

                You can reuse assets previous generated by running python code to duplicate the object, without creating another generation task.
        4. Hunyuan3D
            Hunyuan3D is good at generating 3D models for single item.
            So don't try to:
            1. Generate the whole scene with one shot
            2. Generate ground using Hunyuan3D
            3. Generate parts of the items separately and put them together afterwards

            Use get_hunyuan3d_status() to verify its status
            If Hunyuan3D is enabled:
                if Hunyuan3D mode is "OFFICIAL_API":
                    - For objects/models, do the following steps:
                        1. Create the model generation task
                            - Use generate_hunyuan3d_model by providing either a **text description** OR an **image(local or urls) reference**.
                            - Go to cloud.tencent.com out how to get their own SecretId and SecretKey
                        2. Poll the status
                            - Use poll_hunyuan_job_status() to check if the generation task has completed or failed
                        3. Import the asset
                            - Use import_generated_asset_hunyuan() to import the generated OBJ model the asset
                    if Hunyuan3D mode is "LOCAL_API":
                        - For objects/models, do the following steps:
                        1. Create the model generation task
                            - Use generate_hunyuan3d_model if image (local or urls)  or text prompt is given and import the asset

                You can reuse assets previous generated by running python code to duplicate the object, without creating another generation task.

    3. Always check the world_bounding_box for each item so that:
        - Ensure that all objects that should not be clipping are not clipping.
        - Items have right spatial relationship.
    
    4. Recommended asset source priority:
        - For specific existing objects: First try Sketchfab, then PolyHaven
        - For generic objects/furniture: First try PolyHaven, then Sketchfab
        - For custom or unique items not available in libraries: Use Hyper3D Rodin or Hunyuan3D
        - For environment lighting: Use PolyHaven HDRIs
        - For materials/textures: Use PolyHaven textures

    Only fall back to scripting when:
    - PolyHaven, Sketchfab, Hyper3D, and Hunyuan3D are all disabled
    - A simple primitive is explicitly requested
    - No suitable asset exists in any of the libraries
    - Hyper3D Rodin or Hunyuan3D failed to generate the desired asset
    - The task specifically requires a basic material/color

    **Best Practices:**
    - Always take a screenshot after completing a task to verify the visual result
    - Always call get_scene_info() after completing a task to verify the changes worked
    - When executing multiple operations, take intermediate screenshots to confirm each step
    - If something looks wrong in the screenshot or scene info, investigate and fix before proceeding
    "#;