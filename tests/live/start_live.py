"""Start the BlenderMCP addon inside a live (GUI) Blender session.

This is the script for the live end-to-end test: it loads the real addon in a
windowed Blender, registers it (which auto-starts the socket server on the
scene's `blendermcp_port`, default 9876), enables all four integrations, and
adds a known test object. Commands are executed on the main thread by the
addon's bpy.app.timers callback, which is exactly the production path used by
MCP clients.

Usage:
    blender --python tests/live/start_live.py -- <absolute/path/to/addon.py>

Blender stays open after the script finishes; the server keeps running until
you stop it (Sidebar > BlenderMCP > Stop Server) or quit Blender.
"""

import importlib.util
import sys

import bpy

addon_path = (
    sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "addon/addon.py"
)

spec = importlib.util.spec_from_file_location("blendermcp_addon", addon_path)
addon = importlib.util.module_from_spec(spec)
spec.loader.exec_module(addon)
addon.register()  # auto-starts the socket server on the scene port (9876)

scene = bpy.context.scene
scene.blendermcp_use_polyhaven = True
scene.blendermcp_use_sketchfab = True
scene.blendermcp_use_hyper3d = True
scene.blendermcp_use_hunyuan3d = True

# A known object for the tools to query, so the live check has something to
# inspect beyond the default startup scene.
if "Cube.001" not in bpy.data.objects:
    bpy.ops.mesh.primitive_cube_add(size=2.0, location=(1.0, 2.0, 3.0))

print(f"LIVE_READY port={scene.blendermcp_port}", flush=True)
