//! MCP server for Blender, ported 1:1 from the Python `blender_mcp/server.py`.
//!
//! The handler exposes every tool the Python server exposed, forwarding each
//! call to the Blender addon over its TCP JSON socket. Telemetry was removed
//! by design: no collection, no consent checks, no uploads.

use rmcp::{
    ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, PromptMessage, Role, ServerCapabilities, ServerInfo},
    prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router,
};

use crate::connection::get_blender_connection;
use crate::tools::{hunyuan, hyper3d, polyhaven, scene, sketchfab};

/// The MCP server handler. `#[tool_router]` / `#[prompt_router]` generate the
/// `tool_router()` / `prompt_router()` associated functions that the
/// `#[tool_handler]` / `#[prompt_handler]` implementations call.
#[derive(Debug, Clone, Default)]
pub struct BlenderServer;

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[tool_router]
impl BlenderServer {
    /// Get detailed information about the current Blender scene
    ///
    /// Parameters:
    /// - user_prompt: The original user prompt that led to this tool call
    #[tool(name = "get_scene_info", description = "Get detailed information about the current Blender scene.")]
    fn get_scene_info(&self, Parameters(req): Parameters<scene::GetSceneInfoRequest>) -> String {
        scene::get_scene_info(&get_blender_connection(), &req)
    }

    /// Get detailed information about a specific object in the Blender scene.
    ///
    /// Parameters:
    /// - object_name: The name of the object to get information about
    /// - user_prompt: The original user prompt that led to this tool call
    #[tool(name = "get_object_info", description = "Get detailed information about a specific object in the Blender scene.")]
    fn get_object_info(&self, Parameters(req): Parameters<scene::GetObjectInfoRequest>) -> String {
        scene::get_object_info(&get_blender_connection(), &req)
    }

    /// Capture a screenshot of the current Blender 3D viewport.
    ///
    /// Parameters:
    /// - max_size: Maximum size in pixels for the largest dimension (default: 1000)
    /// - user_prompt: The original user prompt that led to this tool call
    ///
    /// Returns the screenshot as an Image.
    #[tool(name = "get_viewport_screenshot", description = "Capture a screenshot of the current Blender 3D viewport.")]
    fn get_viewport_screenshot(
        &self,
        Parameters(req): Parameters<scene::GetViewportScreenshotRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        scene::get_viewport_screenshot(&get_blender_connection(), &req)
    }

    /// Execute arbitrary Python code in Blender. Make sure to do it step-by-step
    /// by breaking it into smaller chunks.
    ///
    /// Parameters:
    /// - code: The Python code to execute
    /// - user_prompt: The original user prompt that led to this tool call
    #[tool(name = "execute_blender_code", description = "Execute arbitrary Python code in Blender. Make sure to do it step-by-step by breaking it into smaller chunks.")]
    fn execute_blender_code(
        &self,
        Parameters(req): Parameters<scene::ExecuteBlenderCodeRequest>,
    ) -> String {
        scene::execute_blender_code(&get_blender_connection(), &req)
    }

    /// Check if PolyHaven integration is enabled in Blender.
    /// Returns a message indicating whether PolyHaven features are available.
    #[tool(name = "get_polyhaven_status", description = "Check if PolyHaven integration is enabled in Blender.")]
    fn get_polyhaven_status(
        &self,
        Parameters(req): Parameters<polyhaven::GetPolyHavenStatusRequest>,
    ) -> String {
        polyhaven::get_polyhaven_status(&get_blender_connection(), &req)
    }

    /// Get a list of categories for a specific asset type on Polyhaven.
    ///
    /// Parameters:
    /// - asset_type: The type of asset to get categories for (hdris, textures, models, all)
    /// - user_prompt: The original user prompt that led to this tool call
    #[tool(name = "get_polyhaven_categories", description = "Get a list of categories for a specific asset type on Polyhaven.")]
    fn get_polyhaven_categories(
        &self,
        Parameters(req): Parameters<polyhaven::GetPolyHavenCategoriesRequest>,
    ) -> String {
        polyhaven::get_polyhaven_categories(&get_blender_connection(), &req)
    }

    /// Search for assets on Polyhaven with optional filtering.
    ///
    /// Parameters:
    /// - asset_type: Type of assets to search for (hdris, textures, models, all)
    /// - categories: Optional comma-separated list of categories to filter by
    /// - user_prompt: The original user prompt that led to this tool call
    ///
    /// Returns a list of matching assets with basic information.
    #[tool(name = "search_polyhaven_assets", description = "Search for assets on Polyhaven with optional filtering.")]
    fn search_polyhaven_assets(
        &self,
        Parameters(req): Parameters<polyhaven::SearchPolyHavenAssetsRequest>,
    ) -> String {
        polyhaven::search_polyhaven_assets(&get_blender_connection(), &req)
    }

    /// Download and import a Polyhaven asset into Blender.
    ///
    /// Parameters:
    /// - asset_id: The ID of the asset to download
    /// - asset_type: The type of asset (hdris, textures, models)
    /// - resolution: The resolution to download (e.g., 1k, 2k, 4k)
    /// - file_format: Optional file format (e.g., hdr, exr for HDRIs; jpg, png for textures; gltf, fbx for models)
    /// - user_prompt: The original user prompt that led to this tool call
    ///
    /// Returns a message indicating success or failure.
    #[tool(name = "download_polyhaven_asset", description = "Download and import a Polyhaven asset into Blender.")]
    fn download_polyhaven_asset(
        &self,
        Parameters(req): Parameters<polyhaven::DownloadPolyHavenAssetRequest>,
    ) -> String {
        polyhaven::download_polyhaven_asset(&get_blender_connection(), &req)
    }

    /// Apply a previously downloaded Polyhaven texture to an object.
    ///
    /// Parameters:
    /// - object_name: Name of the object to apply the texture to
    /// - texture_id: ID of the Polyhaven texture to apply (must be downloaded first)
    ///
    /// Returns a message indicating success or failure.
    #[tool(name = "set_texture", description = "Apply a previously downloaded Polyhaven texture to an object.")]
    fn set_texture(&self, Parameters(req): Parameters<polyhaven::SetTextureRequest>) -> String {
        polyhaven::set_texture(&get_blender_connection(), &req)
    }

    /// Check if Hyper3D Rodin integration is enabled in Blender.
    /// Returns a message indicating whether Hyper3D Rodin features are available.
    #[tool(name = "get_hyper3d_status", description = "Check if Hyper3D Rodin integration is enabled in Blender.")]
    fn get_hyper3d_status(
        &self,
        Parameters(req): Parameters<hyper3d::GetHyper3dStatusRequest>,
    ) -> String {
        hyper3d::get_hyper3d_status(&get_blender_connection(), &req)
    }

    /// Generate 3D asset using Hyper3D by giving description of the desired asset,
    /// and import the asset into Blender.
    /// The 3D asset has built-in materials.
    /// The generated model has a normalized size, so re-scaling after generation
    /// can be useful.
    ///
    /// Parameters:
    /// - text_prompt: A short description of the desired model in **English**.
    /// - bbox_condition: Optional. If given, it has to be a list of floats of length 3. Controls the ratio between [Length, Width, Height] of the model.
    ///
    /// Returns a message indicating success or failure.
    #[tool(name = "generate_hyper3d_model_via_text", description = "Generate 3D asset using Hyper3D by giving description of the desired asset, and import the asset into Blender.")]
    fn generate_hyper3d_model_via_text(
        &self,
        Parameters(req): Parameters<hyper3d::GenerateHyper3dModelViaTextRequest>,
    ) -> String {
        hyper3d::generate_hyper3d_model_via_text(&get_blender_connection(), &req)
    }

    /// Generate 3D asset using Hyper3D by giving images of the wanted asset, and
    /// import the generated asset into Blender.
    /// The 3D asset has built-in materials.
    /// The generated model has a normalized size, so re-scaling after generation
    /// can be useful.
    ///
    /// Parameters:
    /// - input_image_paths: The **absolute** paths of input images. Even if only one image is provided, wrap it into a list. Required if Hyper3D Rodin in MAIN_SITE mode.
    /// - input_image_urls: The URLs of input images. Even if only one image is provided, wrap it into a list. Required if Hyper3D Rodin in FAL_AI mode.
    /// - bbox_condition: Optional. If given, it has to be a list of ints of length 3. Controls the ratio between [Length, Width, Height] of the model.
    ///
    /// Only one of {input_image_paths, input_image_urls} should be given at a time,
    /// depending on the Hyper3D Rodin's current mode.
    /// Returns a message indicating success or failure.
    #[tool(name = "generate_hyper3d_model_via_images", description = "Generate 3D asset using Hyper3D by giving images of the wanted asset, and import the generated asset into Blender.")]
    fn generate_hyper3d_model_via_images(
        &self,
        Parameters(req): Parameters<hyper3d::GenerateHyper3dModelViaImagesRequest>,
    ) -> String {
        hyper3d::generate_hyper3d_model_via_images(&get_blender_connection(), &req)
    }

    /// Check if the Hyper3D Rodin generation task is completed.
    ///
    /// For Hyper3D Rodin mode MAIN_SITE:
    /// - subscription_key: The subscription_key given in the generate model step.
    ///
    /// Returns a list of status. The task is done if all status are "Done".
    /// If "Failed" showed up, the generating process failed.
    /// This is a polling API, so only proceed if the status are finally determined ("Done" or "Canceled").
    ///
    /// For Hyper3D Rodin mode FAL_AI:
    /// - request_id: The request_id given in the generate model step.
    ///
    /// Returns the generation task status. The task is done if status is "COMPLETED".
    /// The task is in progress if status is "IN_PROGRESS".
    /// If status other than "COMPLETED", "IN_PROGRESS", "IN_QUEUE" showed up, the generating process might be failed.
    /// This is a polling API, so only proceed if the status are finally determined ("COMPLETED" or some failed state).
    #[tool(name = "poll_rodin_job_status", description = "Check if the Hyper3D Rodin generation task is completed.")]
    fn poll_rodin_job_status(
        &self,
        Parameters(req): Parameters<hyper3d::PollRodinJobStatusRequest>,
    ) -> String {
        hyper3d::poll_rodin_job_status(&get_blender_connection(), &req)
    }

    /// Import the asset generated by Hyper3D Rodin after the generation task is completed.
    ///
    /// Parameters:
    /// - name: The name of the object in scene
    /// - task_uuid: For Hyper3D Rodin mode MAIN_SITE: The task_uuid given in the generate model step.
    /// - request_id: For Hyper3D Rodin mode FAL_AI: The request_id given in the generate model step.
    ///
    /// Only give one of {task_uuid, request_id} based on the Hyper3D Rodin Mode!
    /// Return if the asset has been imported successfully.
    #[tool(name = "import_generated_asset", description = "Import the asset generated by Hyper3D Rodin after the generation task is completed.")]
    fn import_generated_asset(
        &self,
        Parameters(req): Parameters<hyper3d::ImportGeneratedAssetRequest>,
    ) -> String {
        hyper3d::import_generated_asset(&get_blender_connection(), &req)
    }

    /// Check if Sketchfab integration is enabled in Blender.
    /// Returns a message indicating whether Sketchfab features are available.
    #[tool(name = "get_sketchfab_status", description = "Check if Sketchfab integration is enabled in Blender.")]
    fn get_sketchfab_status(
        &self,
        Parameters(req): Parameters<sketchfab::GetSketchfabStatusRequest>,
    ) -> String {
        sketchfab::get_sketchfab_status(&get_blender_connection(), &req)
    }

    /// Search for models on Sketchfab with optional filtering.
    ///
    /// Parameters:
    /// - query: Text to search for
    /// - categories: Optional comma-separated list of categories
    /// - count: Maximum number of results to return (default 20)
    /// - downloadable: Whether to include only downloadable models (default True)
    ///
    /// Returns a formatted list of matching models.
    #[tool(name = "search_sketchfab_models", description = "Search for models on Sketchfab with optional filtering.")]
    fn search_sketchfab_models(
        &self,
        Parameters(req): Parameters<sketchfab::SearchSketchfabModelsRequest>,
    ) -> String {
        sketchfab::search_sketchfab_models(&get_blender_connection(), &req)
    }

    /// Get a preview thumbnail of a Sketchfab model by its UID.
    /// Use this to visually confirm a model before downloading.
    ///
    /// Parameters:
    /// - uid: The unique identifier of the Sketchfab model (obtained from search_sketchfab_models)
    ///
    /// Returns the model's thumbnail as an Image for visual confirmation.
    #[tool(name = "get_sketchfab_model_preview", description = "Get a preview thumbnail of a Sketchfab model by its UID.")]
    fn get_sketchfab_model_preview(
        &self,
        Parameters(req): Parameters<sketchfab::GetSketchfabModelPreviewRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        sketchfab::get_sketchfab_model_preview(&get_blender_connection(), &req)
    }

    /// Download and import a Sketchfab model by its UID.
    /// The model will be scaled so its largest dimension equals target_size.
    ///
    /// Parameters:
    /// - uid: The unique identifier of the Sketchfab model
    /// - target_size: REQUIRED. The target size in Blender units/meters for the largest dimension.
    ///               You must specify the desired size for the model.
    ///               Examples:
    ///               - Chair: target_size=1.0 (1 meter tall)
    ///               - Table: target_size=0.75 (75cm tall)
    ///               - Car: target_size=4.5 (4.5 meters long)
    ///               - Person: target_size=1.7 (1.7 meters tall)
    ///               - Small object (cup, phone): target_size=0.1 to 0.3
    ///
    /// Returns a message with import details including object names, dimensions, and bounding box.
    /// The model must be downloadable and you must have proper access rights.
    #[tool(name = "download_sketchfab_model", description = "Download and import a Sketchfab model by its UID.")]
    fn download_sketchfab_model(
        &self,
        Parameters(req): Parameters<sketchfab::DownloadSketchfabModelRequest>,
    ) -> String {
        sketchfab::download_sketchfab_model(&get_blender_connection(), &req)
    }

    /// Check if Hunyuan3D integration is enabled in Blender.
    /// Returns a message indicating whether Hunyuan3D features are available.
    #[tool(name = "get_hunyuan3d_status", description = "Check if Hunyuan3D integration is enabled in Blender.")]
    fn get_hunyuan3d_status(
        &self,
        Parameters(req): Parameters<hunyuan::GetHunyuan3dStatusRequest>,
    ) -> String {
        hunyuan::get_hunyuan3d_status(&get_blender_connection(), &req)
    }

    /// Generate 3D asset using Hunyuan3D by providing either text description,
    /// image reference, or both for the desired asset, and import the asset into Blender.
    /// The 3D asset has built-in materials.
    ///
    /// Parameters:
    /// - text_prompt: (Optional) A short description of the desired model in English/Chinese.
    /// - input_image_url: (Optional) The local or remote url of the input image. Accepts None if only using text prompt.
    ///
    /// Returns:
    /// - When successful, returns a JSON with job_id (format: "job_xxx") indicating the task is in progress
    /// - When the job completes, the status will change to "DONE" indicating the model has been imported
    /// - Returns error message if the operation fails
    #[tool(name = "generate_hunyuan3d_model", description = "Generate 3D asset using Hunyuan3D by providing either text description, image reference, or both for the desired asset, and import the asset into Blender.")]
    fn generate_hunyuan3d_model(
        &self,
        Parameters(req): Parameters<hunyuan::GenerateHunyuan3dModelRequest>,
    ) -> String {
        hunyuan::generate_hunyuan3d_model(&get_blender_connection(), &req)
    }

    /// Check if the Hunyuan3D generation task is completed.
    ///
    /// For Hunyuan3D:
    /// - job_id: The job_id given in the generate model step.
    ///
    /// Returns the generation task status. The task is done if status is "DONE".
    /// The task is in progress if status is "RUN".
    /// If status is "DONE", returns ResultFile3Ds, which is the generated ZIP model path
    /// When the status is "DONE", the response includes a field named ResultFile3Ds that contains the generated ZIP file path of the 3D model in OBJ format.
    /// This is a polling API, so only proceed if the status are finally determined ("DONE" or some failed state).
    #[tool(name = "poll_hunyuan_job_status", description = "Check if the Hunyuan3D generation task is completed.")]
    fn poll_hunyuan_job_status(
        &self,
        Parameters(req): Parameters<hunyuan::PollHunyuanJobStatusRequest>,
    ) -> String {
        hunyuan::poll_hunyuan_job_status(&get_blender_connection(), &req)
    }

    /// Import the asset generated by Hunyuan3D after the generation task is completed.
    ///
    /// Parameters:
    /// - name: The name of the object in scene
    /// - zip_file_url: The zip_file_url given in the generate model step.
    ///
    /// Return if the asset has been imported successfully.
    #[tool(name = "import_generated_asset_hunyuan", description = "Import the asset generated by Hunyuan3D after the generation task is completed.")]
    fn import_generated_asset_hunyuan(
        &self,
        Parameters(req): Parameters<hunyuan::ImportGeneratedAssetHunyuanRequest>,
    ) -> String {
        hunyuan::import_generated_asset_hunyuan(&get_blender_connection(), &req)
    }
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

#[prompt_router]
impl BlenderServer {
    /// Defines the preferred strategy for creating assets in Blender
    #[prompt(name = "asset_creation_strategy", description = "Defines the preferred strategy for creating assets in Blender")]
    fn asset_creation_strategy(&self) -> Result<Vec<PromptMessage>, rmcp::ErrorData> {
        Ok(vec![PromptMessage::new_text(
            Role::User,
            hunyuan::asset_creation_strategy(),
        )])
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[tool_handler]
#[prompt_handler]
impl ServerHandler for BlenderServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_instructions(
            "Blender integration through the Model Context Protocol. \
             Start with get_scene_info() to see the current scene, use \
             get_viewport_screenshot() to verify visual results, and prefer \
             the PolyHaven / Sketchfab / Hyper3D / Hunyuan3D integrations \
             before falling back to execute_blender_code(). See the \
             asset_creation_strategy prompt for the full workflow.",
        )
    }
}