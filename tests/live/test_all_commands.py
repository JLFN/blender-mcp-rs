"""Comprehensive live test: exercises all 22 command handlers against a live Blender.

This is the QA test to verify the 1:1 port matches the Python addon's capabilities.
Run from inside Blender after the addon is loaded:
    blender --python tests/live/test_all_commands.py -- <addon_path>
"""

import sys
import json
import importlib.util
import bpy
import socket
import tempfile

addon_path = sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "addon/addon.py"

spec = importlib.util.spec_from_file_location("blendermcp_addon", addon_path)
addon = importlib.util.module_from_spec(spec)
spec.loader.exec_module(addon)
addon.register()

scene = bpy.context.scene
scene.blendermcp_use_polyhaven = True
scene.blendermcp_use_sketchfab = True
scene.blendermcp_use_hyper3d = True
scene.blendermcp_use_hunyuan3d = True

# Create test object if needed
if "Cube.001" not in bpy.data.objects:
    bpy.ops.mesh.primitive_cube_add(size=2.0, location=(1.0, 2.0, 3.0))

# Use the server directly like the driver does
srv = addon.BlenderMCPServer("127.0.0.1", 0)  # port 0 = not used for this test

print("=" * 60)
print("COMPREHENSIVE LIVE TEST - All 22 Command Handlers")
print("=" * 60)

results = {}

# ============================================================
# BASE COMMANDS (always available)
# ============================================================

# 1. get_scene_info
print("\n[1/22] get_scene_info...")
result = srv.execute_command({"type": "get_scene_info", "params": {}})
results["get_scene_info"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "success":
    obj_count = result.get("result", {}).get("object_count", 0)
    print(f"  Objects: {obj_count}")

# 2. get_object_info
print("\n[2/22] get_object_info...")
result = srv.execute_command({"type": "get_object_info", "params": {"name": "Cube.001"}})
results["get_object_info"] = result.get("status")
print(f"  Status: {result.get('status')}")

# 3. get_viewport_screenshot
print("\n[3/22] get_viewport_screenshot...")
with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
    tmp_path = f.name
result = srv.execute_command({"type": "get_viewport_screenshot", "params": {"max_size": 256, "filepath": tmp_path}})
results["get_viewport_screenshot"] = result.get("status")
print(f"  Status: {result.get('status')}")

# 4. execute_code
print("\n[4/22] execute_code...")
result = srv.execute_command({"type": "execute_code", "params": {"code": "import bpy\nprint('EXECUTE_CODE_OK')\n"}})
results["execute_code"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "success":
    print(f"  Output: {result.get('result', {}).get('result', '')[:50]}")

# 5. get_polyhaven_status
print("\n[5/22] get_polyhaven_status...")
result = srv.execute_command({"type": "get_polyhaven_status", "params": {}})
results["get_polyhaven_status"] = result.get("status")
print(f"  Status: {result.get('status')}")

# 6. get_hyper3d_status
print("\n[6/22] get_hyper3d_status...")
result = srv.execute_command({"type": "get_hyper3d_status", "params": {}})
results["get_hyper3d_status"] = result.get("status")
print(f"  Status: {result.get('status')}")

# 7. get_sketchfab_status
print("\n[7/22] get_sketchfab_status...")
result = srv.execute_command({"type": "get_sketchfab_status", "params": {}})
results["get_sketchfab_status"] = result.get("status")
print(f"  Status: {result.get('status')}")

# 8. get_hunyuan3d_status
print("\n[8/22] get_hunyuan3d_status...")
result = srv.execute_command({"type": "get_hunyuan3d_status", "params": {}})
results["get_hunyuan3d_status"] = result.get("status")
print(f"  Status: {result.get('status')}")

# 9. get_telemetry_consent
print("\n[9/22] get_telemetry_consent...")
result = srv.execute_command({"type": "get_telemetry_consent", "params": {}})
results["get_telemetry_consent"] = result.get("status")
print(f"  Status: {result.get('status')}")

# ============================================================
# POLYHAVEN COMMANDS (requires blendermcp_use_polyhaven = True)
# ============================================================

# 10. get_polyhaven_categories
print("\n[10/22] get_polyhaven_categories...")
result = srv.execute_command({"type": "get_polyhaven_categories", "params": {"asset_type": "hdris"}})
results["get_polyhaven_categories"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "success":
    cats = result.get("result", {}).get("categories", {})
    print(f"  Categories: {len(cats)}")

# 11. search_polyhaven_assets
print("\n[11/22] search_polyhaven_assets...")
result = srv.execute_command({"type": "search_polyhaven_assets", "params": {"asset_type": "textures", "categories": "metal"}})
results["search_polyhaven_assets"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "success":
    assets = result.get("result", {}).get("assets", {})
    print(f"  Assets found: {len(assets)}")

# 12. download_polyhaven_asset
print("\n[12/22] download_polyhaven_asset...")
result = srv.execute_command({"type": "download_polyhaven_asset", "params": {"asset_id": "metal_plate", "asset_type": "textures", "resolution": "1k", "file_format": "jpg"}})
results["download_polyhaven_asset"] = result.get("status")
print(f"  Status: {result.get('status')}")

# 13. set_texture
print("\n[13/22] set_texture...")
result = srv.execute_command({"type": "set_texture", "params": {"object_name": "Cube.001", "texture_id": "metal_plate"}})
results["set_texture"] = result.get("status")
print(f"  Status: {result.get('status')}")

# ============================================================
# HYPER3D COMMANDS (requires blendermcp_use_hyper3d = True)
# ============================================================

# 14. create_rodin_job (via text)
print("\n[14/22] create_rodin_job...")
result = srv.execute_command({"type": "create_rodin_job", "params": {"text_prompt": "a simple metal tube"}})
results["create_rodin_job"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "error":
    print(f"  Expected (needs API key): {result.get('message', '')[:80]}")

# 15. poll_rodin_job_status
print("\n[15/22] poll_rodin_job_status...")
result = srv.execute_command({"type": "poll_rodin_job_status", "params": {"subscription_key": "test"}})
results["poll_rodin_job_status"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "error":
    print(f"  Expected (invalid key): {result.get('message', '')[:80]}")

# 16. import_generated_asset
print("\n[16/22] import_generated_asset...")
result = srv.execute_command({"type": "import_generated_asset", "params": {"name": "test", "task_uuid": "test"}})
results["import_generated_asset"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "error":
    print(f"  Expected (invalid task): {result.get('message', '')[:80]}")

# ============================================================
# SKETCHFAB COMMANDS (requires blendermcp_use_sketchfab = True)
# ============================================================

# 17. search_sketchfab_models
print("\n[17/22] search_sketchfab_models...")
result = srv.execute_command({"type": "search_sketchfab_models", "params": {"query": "tube", "count": 3}})
results["search_sketchfab_models"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "success":
    models = result.get("result", {}).get("models", [])
    print(f"  Models found: {len(models)}")

# 18. get_sketchfab_model_preview
print("\n[18/22] get_sketchfab_model_preview...")
result = srv.execute_command({"type": "get_sketchfab_model_preview", "params": {"uid": "c0dd417a4f5444c9a2bcf1dc2eb084c7"}})
results["get_sketchfab_model_preview"] = result.get("status")
print(f"  Status: {result.get('status')}")

# 19. download_sketchfab_model
print("\n[19/22] download_sketchfab_model...")
result = srv.execute_command({"type": "download_sketchfab_model", "params": {"uid": "c0dd417a4f5444c9a2bcf1dc2eb084c7", "target_size": 1.0}})
results["download_sketchfab_model"] = result.get("status")
print(f"  Status: {result.get('status')}")

# ============================================================
# HUNYUAN3D COMMANDS (requires blendermcp_use_hunyuan3d = True)
# ============================================================

# 20. create_hunyuan_job
print("\n[20/22] create_hunyuan_job...")
result = srv.execute_command({"type": "create_hunyuan_job", "params": {"text_prompt": "a simple metal tube"}})
results["create_hunyuan_job"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "error":
    print(f"  Expected (local API not running): {result.get('message', '')[:80]}")

# 21. poll_hunyuan_job_status
print("\n[21/22] poll_hunyuan_job_status...")
result = srv.execute_command({"type": "poll_hunyuan_job_status", "params": {"job_id": "job_test"}})
results["poll_hunyuan_job_status"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "error":
    print(f"  Expected (invalid job): {result.get('message', '')[:80]}")

# 22. import_generated_asset_hunyuan
print("\n[22/22] import_generated_asset_hunyuan...")
result = srv.execute_command({"type": "import_generated_asset_hunyuan", "params": {"name": "test", "zip_file_url": "http://test"}})
results["import_generated_asset_hunyuan"] = result.get("status")
print(f"  Status: {result.get('status')}")
if result.get("status") == "error":
    print(f"  Expected (invalid URL): {result.get('message', '')[:80]}")

# ============================================================
# SUMMARY
# ============================================================
print("\n" + "=" * 60)
print("SUMMARY")
print("=" * 60)

success_count = sum(1 for s in results.values() if s == "success")
error_count = sum(1 for s in results.values() if s == "error")
total = len(results)

for cmd, status in results.items():
    icon = "✓" if status == "success" else "✗"
    print(f"  {icon} {cmd}: {status}")

print(f"\nTotal: {total} | Success: {success_count} | Error: {error_count}")

# Note: Some errors are EXPECTED (Hyper3D needs API key, Hunyuan3D needs local API)
# The important thing is that the handlers EXIST and RESPOND (not "Unknown command type")

# Check for unknown commands
unknown = [cmd for cmd, status in results.items() if status == "error" and "Unknown command type" in str(srv.execute_command({"type": cmd, "params": {}}).get("message", ""))]
if unknown:
    print(f"\n⚠️  UNKNOWN COMMANDS (handler missing!): {unknown}")
else:
    print("\n✓ All 22 command handlers exist and respond")

# Exit code for CI
import sys
sys.exit(0 if not unknown else 1)