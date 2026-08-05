//! Tool-level tests: each tool function is exercised against the mock addon
//! server and its formatted output asserted 1:1 against the Python original's
//! strings. The params the mock receives are also asserted, verifying that
//! `user_prompt` and other MCP-layer fields never leak into the addon payload.

mod support;

use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use blender_mcp_rs::connection::BlenderConnection;
use blender_mcp_rs::tools::{
    hunyuan::{
        GenerateHunyuan3dModelRequest, GetHunyuan3dStatusRequest, ImportGeneratedAssetHunyuanRequest,
        PollHunyuanJobStatusRequest,
    },
    hyper3d::{
        GenerateHyper3dModelViaImagesRequest, GenerateHyper3dModelViaTextRequest,
        ImportGeneratedAssetRequest, PollRodinJobStatusRequest,
    },
    polyhaven::{
        DownloadPolyHavenAssetRequest, GetPolyHavenCategoriesRequest, GetPolyHavenStatusRequest,
        SearchPolyHavenAssetsRequest, SetTextureRequest,
    },
    scene::{
        ExecuteBlenderCodeRequest, GetObjectInfoRequest, GetSceneInfoRequest,
        GetViewportScreenshotRequest,
    },
    sketchfab::{
        DownloadSketchfabModelRequest, GetSketchfabModelPreviewRequest, GetSketchfabStatusRequest,
        SearchSketchfabModelsRequest,
    },
};
use rmcp::model::{CallToolResult, ContentBlock};
use support::MockBlender;

fn conn(port: u16) -> BlenderConnection {
    let mut c = BlenderConnection::new("127.0.0.1", port);
    c.set_timeout(Duration::from_secs(10));
    c
}

// ---------------------------------------------------------------- scene tools

#[test]
fn get_scene_info_returns_pretty_json() {
    let mock = MockBlender::new();
    mock.respond(
        "get_scene_info",
        json!({ "status": "success", "result": { "name": "Scene", "object_count": 1 } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::scene::get_scene_info(
        &c,
        &GetSceneInfoRequest { user_prompt: Some("ignored".into()) },
    );

    // json.dumps(result, indent=2) shape.
    assert_eq!(
        out,
        "{\n  \"name\": \"Scene\",\n  \"object_count\": 1\n}"
    );
    // user_prompt must not reach the addon: params serialize as {}.
    assert_eq!(mock.params_of("get_scene_info"), vec![json!({})]);
}

#[test]
fn get_scene_info_error_is_formatted() {
    let mock = MockBlender::new();
    mock.respond(
        "get_scene_info",
        json!({ "status": "error", "message": "boom" }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::scene::get_scene_info(
        &c,
        &GetSceneInfoRequest::default(),
    );
    assert_eq!(out, "Error getting scene info: boom");
}

#[test]
fn get_object_info_sends_only_name_param() {
    let mock = MockBlender::new();
    mock.respond(
        "get_object_info",
        json!({ "status": "success", "result": { "name": "Cube.001", "type": "MESH" } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::scene::get_object_info(
        &c,
        &GetObjectInfoRequest {
            object_name: "Cube.001".into(),
            user_prompt: Some("make it pretty".into()),
        },
    );

    assert!(out.contains("\"name\": \"Cube.001\""));
    assert_eq!(
        mock.params_of("get_object_info"),
        vec![json!({ "name": "Cube.001" })]
    );
}

#[test]
fn execute_blender_code_returns_captured_output() {
    let mock = MockBlender::new();
    mock.respond(
        "execute_code",
        json!({ "status": "success", "result": { "executed": true, "result": "HELLO\n" } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::scene::execute_blender_code(
        &c,
        &ExecuteBlenderCodeRequest {
            code: "print('HELLO')".into(),
            user_prompt: None,
        },
    );

    assert_eq!(out, "Code executed successfully: HELLO\n");
    assert_eq!(
        mock.params_of("execute_code"),
        vec![json!({ "code": "print('HELLO')" })]
    );
}

#[test]
fn get_viewport_screenshot_returns_image_block() {
    let mock = MockBlender::new();
    mock.respond(
        "get_viewport_screenshot",
        json!({ "status": "success", "result": {} }),
    );

    // The tool reads the PNG it asked Blender to render; plant it first.
    let temp_path = std::env::temp_dir().join(format!(
        "blender_screenshot_{}.png",
        std::process::id()
    ));
    let png_bytes: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
    std::fs::write(&temp_path, &png_bytes).unwrap();

    let c = conn(mock.port());
    let result = blender_mcp_rs::tools::scene::get_viewport_screenshot(
        &c,
        &GetViewportScreenshotRequest {
            max_size: 800,
            user_prompt: None,
        },
    )
    .unwrap();

    assert!(!temp_path.exists(), "temp screenshot must be cleaned up");
    let CallToolResult { content, .. } = result;
    assert_eq!(content.len(), 1);
    match &content[0] {
        ContentBlock::Image(img) => {
            assert_eq!(img.mime_type, "image/png");
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&img.data)
                .unwrap();
            assert_eq!(decoded, png_bytes);
        }
        other => panic!("expected image block, got {other:?}"),
    }
    // The addon was told to render PNG to the exact temp path.
    assert_eq!(
        mock.params_of("get_viewport_screenshot"),
        vec![json!({
            "max_size": 800,
            "filepath": temp_path.to_string_lossy(),
            "format": "png",
        })]
    );
}

// ------------------------------------------------------------- polyhaven tools

#[test]
fn polyhaven_status_appends_texture_hint_when_enabled() {
    let mock = MockBlender::new();
    // The addon's enabled message ends with a period; server.py appends the
    // hint directly with no separator (f-string concatenation), so the two
    // sentences run together exactly as in the Python original.
    mock.respond(
        "get_polyhaven_status",
        json!({ "status": "success", "result": {
            "enabled": true,
            "message": "PolyHaven integration is enabled and ready to use.",
        } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::polyhaven::get_polyhaven_status(
        &c,
        &GetPolyHavenStatusRequest::default(),
    );
    assert_eq!(
        out,
        "PolyHaven integration is enabled and ready to use.PolyHaven is good at Textures, and has a wider variety of textures than Sketchfab."
    );
}

#[test]
fn polyhaven_categories_checks_status_then_formats_sorted() {
    let mock = MockBlender::new();
    mock.respond(
        "get_polyhaven_status",
        json!({ "status": "success", "result": { "enabled": true, "message": "" } }),
    );
    mock.respond(
        "get_polyhaven_categories",
        json!({ "status": "success", "result": { "categories": { "a": 3, "b": 9, "c": 3 } } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::polyhaven::get_polyhaven_categories(
        &c,
        &GetPolyHavenCategoriesRequest {
            asset_type: "hdris".into(),
            user_prompt: None,
        },
    );

    // Stable sort by count desc: ties keep insertion order (a before c).
    assert_eq!(
        out,
        "Categories for hdris:\n\n- b: 9 assets\n- a: 3 assets\n- c: 3 assets\n"
    );
    assert_eq!(
        mock.params_of("get_polyhaven_categories"),
        vec![json!({ "asset_type": "hdris" })]
    );
}

#[test]
fn polyhaven_categories_disabled_returns_hint() {
    let mock = MockBlender::new();
    mock.respond(
        "get_polyhaven_status",
        json!({ "status": "success", "result": { "enabled": false, "message": "" } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::polyhaven::get_polyhaven_categories(
        &c,
        &GetPolyHavenCategoriesRequest::default(),
    );
    assert_eq!(
        out,
        "PolyHaven integration is disabled. Select it in the sidebar in BlenderMCP, then run it again."
    );
    assert!(mock.params_of("get_polyhaven_categories").is_empty());
}

#[test]
fn polyhaven_search_formats_assets_sorted_by_downloads() {
    let mock = MockBlender::new();
    mock.respond(
        "search_polyhaven_assets",
        json!({
            "status": "success",
            "result": {
                "total_count": 3,
                "returned_count": 2,
                "assets": {
                    "id_1": { "name": "Rock", "type": 2, "categories": ["Rock"], "download_count": 50 },
                    "id_0": { "name": "Sky", "type": 0, "categories": ["Sky", "HDRI"], "download_count": 99 },
                },
            }
        }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::polyhaven::search_polyhaven_assets(
        &c,
        &SearchPolyHavenAssetsRequest {
            asset_type: "hdris".into(),
            categories: Some("Sky".into()),
            user_prompt: None,
        },
    );

    assert_eq!(
        out,
        "Found 3 assets in categories: Sky\nShowing 2 assets:\n\n\
         - Sky (ID: id_0)\n  Type: HDRI\n  Categories: Sky, HDRI\n  Downloads: 99\n\n\
         - Rock (ID: id_1)\n  Type: Model\n  Categories: Rock\n  Downloads: 50\n\n"
    );
    assert_eq!(
        mock.params_of("search_polyhaven_assets"),
        vec![json!({ "asset_type": "hdris", "categories": "Sky" })]
    );
}

#[test]
fn polyhaven_download_hdri_appends_environment_note() {
    let mock = MockBlender::new();
    // The addon's success message carries no trailing period; server.py always
    // appends ". The HDRI..." to it.
    mock.respond(
        "download_polyhaven_asset",
        json!({ "status": "success", "result": { "success": true, "message": "Asset downloaded and imported successfully" } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::polyhaven::download_polyhaven_asset(
        &c,
        &DownloadPolyHavenAssetRequest {
            asset_id: "asset_x".into(),
            asset_type: "hdris".into(),
            resolution: "1k".into(),
            file_format: None,
            user_prompt: None,
        },
    );
    assert_eq!(
        out,
        "Asset downloaded and imported successfully. The HDRI has been set as the world environment."
    );
    assert_eq!(
        mock.params_of("download_polyhaven_asset"),
        vec![json!({
            "asset_id": "asset_x",
            "asset_type": "hdris",
            "resolution": "1k",
            "file_format": null,
        })]
    );
}

#[test]
fn polyhaven_download_texture_formats_material() {
    let mock = MockBlender::new();
    mock.respond(
        "download_polyhaven_asset",
        json!({ "status": "success", "result": {
            "success": true, "message": "Asset downloaded and imported successfully", "material": "Mat", "maps": ["diffuse", "rough"]
        } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::polyhaven::download_polyhaven_asset(
        &c,
        &DownloadPolyHavenAssetRequest {
            asset_id: "t1".into(),
            asset_type: "textures".into(),
            resolution: "1k".into(),
            file_format: Some("jpg".into()),
            user_prompt: None,
        },
    );
    assert_eq!(
        out,
        "Asset downloaded and imported successfully. Created material 'Mat' with maps: diffuse, rough."
    );
}

#[test]
fn polyhaven_set_texture_formats_material_info() {
    let mock = MockBlender::new();
    mock.respond(
        "set_texture",
        json!({ "status": "success", "result": {
            "success": true,
            "material": "Mat",
            "maps": ["diffuse"],
            "material_info": {
                "has_nodes": true,
                "node_count": 5,
                "texture_nodes": [
                    { "name": "Image Texture", "image": "rock_diffuse", "connections": ["Base Color"] },
                    { "name": "Plain", "image": "", "connections": [] },
                ],
            },
        } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::polyhaven::set_texture(
        &c,
        &SetTextureRequest {
            object_name: "Cube".into(),
            texture_id: "rock".into(),
            user_prompt: None,
        },
    );

    assert_eq!(
        out,
        "Successfully applied texture 'rock' to Cube.\n\
         Using material 'Mat' with maps: diffuse.\n\n\
         Material has nodes: true\n\
         Total node count: 5\n\n\
         Texture nodes:\n\
         - Image Texture using image: rock_diffuse\n  Connections:\n    Base Color\n\
         - Plain using image: \n"
    );
}

// ------------------------------------------------------------ sketchfab tools

#[test]
fn sketchfab_status_appends_realistic_hint_when_enabled() {
    let mock = MockBlender::new();
    // Same direct-concatenation behavior as PolyHaven status.
    mock.respond(
        "get_sketchfab_status",
        json!({ "status": "success", "result": {
            "enabled": true,
            "message": "Sketchfab integration is enabled and ready to use.",
        } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::sketchfab::get_sketchfab_status(
        &c,
        &GetSketchfabStatusRequest::default(),
    );
    assert_eq!(
        out,
        "Sketchfab integration is enabled and ready to use.Sketchfab is good at Realistic models, and has a wider variety of models than PolyHaven."
    );
}

#[test]
fn sketchfab_search_formats_model_entries() {
    let mock = MockBlender::new();
    mock.respond(
        "search_sketchfab_models",
        json!({ "status": "success", "result": { "results": [
            {
                "name": "Chair", "uid": "u1",
                "user": { "username": "alice" },
                "license": { "label": "CC-BY" },
                "faceCount": 1200,
                "isDownloadable": true,
            },
            {
                "name": "Lamp", "uid": "u2",
                "user": null,
                "license": null,
                "isDownloadable": false,
            },
        ] } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::sketchfab::search_sketchfab_models(
        &c,
        &SearchSketchfabModelsRequest {
            query: "chair".into(),
            categories: None,
            count: 20,
            downloadable: true,
            user_prompt: None,
        },
    );

    assert_eq!(
        out,
        "Found 2 models matching 'chair':\n\n\
         - Chair (UID: u1)\n  Author: alice\n  License: CC-BY\n  Face count: 1200\n  Downloadable: Yes\n\n\
         - Lamp (UID: u2)\n  Author: Unknown author\n  License: Unknown\n  Face count: Unknown\n  Downloadable: No\n\n"
    );
    assert_eq!(
        mock.params_of("search_sketchfab_models"),
        vec![json!({
            "query": "chair", "categories": null, "count": 20, "downloadable": true
        })]
    );
}

#[test]
fn sketchfab_preview_returns_image_with_correct_mime() {
    let mock = MockBlender::new();
    let png_b64 = base64::engine::general_purpose::STANDARD
        .encode([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 9, 9, 9]);
    mock.respond(
        "get_sketchfab_model_preview",
        json!({ "status": "success", "result": {
            "image_data": png_b64,
            "format": "png",
            "model_name": "Chair",
            "author": "alice",
        } }),
    );

    let c = conn(mock.port());
    let result = blender_mcp_rs::tools::sketchfab::get_sketchfab_model_preview(
        &c,
        &GetSketchfabModelPreviewRequest { uid: "u1".into(), user_prompt: None },
    )
    .unwrap();

    let CallToolResult { content, .. } = result;
    match &content[0] {
        ContentBlock::Image(img) => {
            assert_eq!(img.mime_type, "image/png");
            assert_eq!(img.data, png_b64);
        }
        other => panic!("expected image block, got {other:?}"),
    }
    assert_eq!(
        mock.params_of("get_sketchfab_model_preview"),
        vec![json!({ "uid": "u1" })]
    );
}

#[test]
fn sketchfab_preview_invalid_base64_is_tool_error() {
    let mock = MockBlender::new();
    mock.respond(
        "get_sketchfab_model_preview",
        json!({ "status": "success", "result": { "image_data": "%%%not-base64%%%", "format": "png" } }),
    );

    let c = conn(mock.port());
    let err = blender_mcp_rs::tools::sketchfab::get_sketchfab_model_preview(
        &c,
        &GetSketchfabModelPreviewRequest { uid: "u1".into(), user_prompt: None },
    )
    .unwrap_err();
    assert!(err.message.contains("invalid image data"), "{err:?}");
}

#[test]
fn sketchfab_download_formats_normalization() {
    let mock = MockBlender::new();
    mock.respond(
        "download_sketchfab_model",
        json!({ "status": "success", "result": {
            "success": true,
            "imported_objects": ["Chair"],
            "dimensions": [0.5, 1.25, 0.75],
            "world_bounding_box": [[0.0, 0.0, 0.0], [0.5, 1.25, 0.75]],
            "normalized": true,
            "scale_applied": 0.123456,
        } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::sketchfab::download_sketchfab_model(
        &c,
        &DownloadSketchfabModelRequest { uid: "u1".into(), target_size: 2.0, user_prompt: None },
    );

    assert_eq!(
        out,
        "Successfully imported model.\n\
         Created objects: Chair\n\
         Dimensions (X, Y, Z): 0.500 x 1.250 x 0.750 meters\n\
         Bounding box: min=[0.0,0.0,0.0], max=[0.5,1.25,0.75]\n\
         Size normalized: scale factor 0.123456 applied (target size: 2m)\n"
    );
    assert_eq!(
        mock.params_of("download_sketchfab_model"),
        vec![json!({
            "uid": "u1", "normalize_size": true, "target_size": 2.0
        })]
    );
}

// -------------------------------------------------------------- hyper3d tools

#[test]
fn hyper3d_generate_via_text_sends_null_images_and_formats_submission() {
    let mock = MockBlender::new();
    mock.respond(
        "create_rodin_job",
        json!({ "status": "success", "result": {
            "submit_time": "2026-08-05T10:00:00Z",
            "uuid": "u-77",
            "jobs": { "subscription_key": "sk-77" },
        } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::hyper3d::generate_hyper3d_model_via_text(
        &c,
        &GenerateHyper3dModelViaTextRequest {
            text_prompt: "a chair".into(),
            bbox_condition: None,
            user_prompt: Some("ignored".into()),
        },
    );

    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        parsed,
        json!({ "task_uuid": "u-77", "subscription_key": "sk-77" })
    );
    // The exact wire dict from the Python original:
    assert_eq!(
        mock.params_of("create_rodin_job"),
        vec![json!({
            "text_prompt": "a chair",
            "images": null,
            "bbox_condition": null,
        })]
    );
}

#[test]
fn hyper3d_generate_via_text_with_bbox_normalizes() {
    let mock = MockBlender::new();
    mock.respond(
        "create_rodin_job",
        json!({ "status": "success", "result": {} }),
    );

    let c = conn(mock.port());
    let _ = blender_mcp_rs::tools::hyper3d::generate_hyper3d_model_via_text(
        &c,
        &GenerateHyper3dModelViaTextRequest {
            text_prompt: "x".into(),
            bbox_condition: Some(vec![1.0, 2.0, 4.0]),
            user_prompt: None,
        },
    );

    assert_eq!(
        mock.params_of("create_rodin_job"),
        vec![json!({
            "text_prompt": "x",
            "images": null,
            "bbox_condition": [25, 50, 100],
        })]
    );
}

#[test]
fn hyper3d_generate_via_images_encodes_local_files() {
    let mock = MockBlender::new();
    mock.respond(
        "create_rodin_job",
        json!({ "status": "success", "result": {
            "submit_time": "t", "uuid": "u-1", "jobs": { "subscription_key": "sk-1" }
        } }),
    );

    let img_bytes = b"\x89PNG fake image bytes";
    let img_path = std::env::temp_dir().join("blender_mcp_rs_test_ref.png");
    std::fs::write(&img_path, img_bytes).unwrap();

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::hyper3d::generate_hyper3d_model_via_images(
        &c,
        &GenerateHyper3dModelViaImagesRequest {
            input_image_paths: Some(vec![img_path.to_string_lossy().to_string()]),
            input_image_urls: None,
            bbox_condition: None,
            user_prompt: None,
        },
    );

    let b64 = base64::engine::general_purpose::STANDARD.encode(img_bytes);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        parsed,
        json!({ "task_uuid": "u-1", "subscription_key": "sk-1" })
    );
    assert_eq!(
        mock.params_of("create_rodin_job"),
        vec![json!({
            "text_prompt": null,
            "images": [[ "png", b64 ]],
            "bbox_condition": null,
        })]
    );
    let _ = std::fs::remove_file(&img_path);
}

#[test]
fn hyper3d_generate_via_images_conflict_and_missing_errors() {
    let mock = MockBlender::new();
    let c = conn(mock.port());

    let both = blender_mcp_rs::tools::hyper3d::generate_hyper3d_model_via_images(
        &c,
        &GenerateHyper3dModelViaImagesRequest {
            input_image_paths: Some(vec![]),
            input_image_urls: Some(vec![]),
            bbox_condition: None,
            user_prompt: None,
        },
    );
    assert_eq!(both, "Error: Conflict parameters given!");

    let neither = blender_mcp_rs::tools::hyper3d::generate_hyper3d_model_via_images(
        &c,
        &GenerateHyper3dModelViaImagesRequest {
            input_image_paths: None,
            input_image_urls: None,
            bbox_condition: None,
            user_prompt: None,
        },
    );
    assert_eq!(neither, "Error: No image given!");

    let bad_path = blender_mcp_rs::tools::hyper3d::generate_hyper3d_model_via_images(
        &c,
        &GenerateHyper3dModelViaImagesRequest {
            input_image_paths: Some(vec!["/nonexistent/nowhere.png".into()]),
            input_image_urls: None,
            bbox_condition: None,
            user_prompt: None,
        },
    );
    assert_eq!(bad_path, "Error: not all image paths are valid!");

    assert!(mock.received().is_empty(), "no command must reach the addon");
}

#[test]
fn hyper3d_poll_and_import_build_kwargs_like_python() {
    let mock = MockBlender::new();
    mock.respond(
        "poll_rodin_job_status",
        json!({ "status": "success", "result": { "status": "COMPLETED" } }),
    );
    mock.respond(
        "import_generated_asset",
        json!({ "status": "success", "result": { "imported": true } }),
    );

    let c = conn(mock.port());

    // subscription_key branch.
    let _ = blender_mcp_rs::tools::hyper3d::poll_rodin_job_status(
        &c,
        &PollRodinJobStatusRequest {
            subscription_key: Some("sk-9".into()),
            request_id: None,
        },
    );
    assert_eq!(
        mock.params_of("poll_rodin_job_status"),
        vec![json!({ "subscription_key": "sk-9" })]
    );

    // request_id branch.
    let _ = blender_mcp_rs::tools::hyper3d::poll_rodin_job_status(
        &c,
        &PollRodinJobStatusRequest {
            subscription_key: None,
            request_id: Some("r-9".into()),
        },
    );
    assert_eq!(
        mock.params_of("poll_rodin_job_status"),
        vec![json!({ "subscription_key": "sk-9" }), json!({ "request_id": "r-9" })]
    );

    // import with task_uuid.
    let _ = blender_mcp_rs::tools::hyper3d::import_generated_asset(
        &c,
        &ImportGeneratedAssetRequest {
            name: "Chair".into(),
            task_uuid: Some("u-9".into()),
            request_id: None,
        },
    );
    assert_eq!(
        mock.params_of("import_generated_asset"),
        vec![json!({ "name": "Chair", "task_uuid": "u-9" })]
    );
}

// -------------------------------------------------------------- hunyuan tools

#[test]
fn hunyuan_status_returns_message() {
    let mock = MockBlender::new();
    mock.respond(
        "get_hunyuan3d_status",
        json!({ "status": "success", "result": { "enabled": true, "message": "Hunyuan3D ready." } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::hunyuan::get_hunyuan3d_status(
        &c,
        &GetHunyuan3dStatusRequest::default(),
    );
    assert_eq!(out, "Hunyuan3D ready.");
}

#[test]
fn hunyuan_generate_formats_job_id() {
    let mock = MockBlender::new();
    mock.respond(
        "create_hunyuan_job",
        json!({ "status": "success", "result": { "Response": { "JobId": "314159" } } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::hunyuan::generate_hunyuan3d_model(
        &c,
        &GenerateHunyuan3dModelRequest {
            text_prompt: Some("a vase".into()),
            input_image_url: None,
            user_prompt: None,
        },
    );

    assert_eq!(out.trim(), "{\n  \"job_id\": \"job_314159\"\n}");
    assert_eq!(
        mock.params_of("create_hunyuan_job"),
        vec![json!({ "text_prompt": "a vase", "image": null })]
    );
}

#[test]
fn hunyuan_poll_and_import_send_exact_params() {
    let mock = MockBlender::new();
    mock.respond(
        "poll_hunyuan_job_status",
        json!({ "status": "success", "result": { "status": "DONE" } }),
    );
    mock.respond(
        "import_generated_asset_hunyuan",
        json!({ "status": "success", "result": { "imported": true } }),
    );

    let c = conn(mock.port());
    let _ = blender_mcp_rs::tools::hunyuan::poll_hunyuan_job_status(
        &c,
        &PollHunyuanJobStatusRequest { job_id: Some("job_1".into()) },
    );
    let _ = blender_mcp_rs::tools::hunyuan::import_generated_asset_hunyuan(
        &c,
        &ImportGeneratedAssetHunyuanRequest {
            name: "Vase".into(),
            zip_file_url: Some("https://example.com/v.zip".into()),
        },
    );

    assert_eq!(
        mock.params_of("poll_hunyuan_job_status"),
        vec![json!({ "job_id": "job_1" })]
    );
    assert_eq!(
        mock.params_of("import_generated_asset_hunyuan"),
        vec![json!({ "name": "Vase", "zip_file_url": "https://example.com/v.zip" })]
    );
}

// ------------------------------------------------------------- error handling

#[test]
fn tool_error_messages_match_python_prefixes() {
    let mock = MockBlender::new();
    mock.respond(
        "get_sketchfab_status",
        json!({ "status": "error", "message": "api down" }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::sketchfab::get_sketchfab_status(
        &c,
        &GetSketchfabStatusRequest::default(),
    );
    assert_eq!(out, "Error checking Sketchfab status: api down");
}

#[test]
fn hyper3d_generate_via_images_with_invalid_url_branch_is_documented_fix() {
    // The Python source validated `input_image_paths` in the URL branch (always
    // None there), making the URL branch always error. The Rust port validates
    // the URLs instead, which is the documented intentional fix.
    let mock = MockBlender::new();
    mock.respond(
        "create_rodin_job",
        json!({ "status": "success", "result": { "submit_time": "t", "uuid": "u", "jobs": { "subscription_key": "s" } } }),
    );

    let c = conn(mock.port());
    let out = blender_mcp_rs::tools::hyper3d::generate_hyper3d_model_via_images(
        &c,
        &GenerateHyper3dModelViaImagesRequest {
            input_image_paths: None,
            input_image_urls: Some(vec!["https://example.com/a.png".into()]),
            bbox_condition: None,
            user_prompt: None,
        },
    );

    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed, json!({ "task_uuid": "u", "subscription_key": "s" }));
    assert_eq!(
        mock.params_of("create_rodin_job"),
        vec![json!({
            "text_prompt": null,
            "images": ["https://example.com/a.png"],
            "bbox_condition": null,
        })]
    );
}
