"""Headless test driver for the BlenderMCP addon.

Runs the real addon (addon/addon.py) inside a headless Blender (blender -b)
and serves its TCP JSON command socket from the main thread. The addon
normally hops command execution to the Blender main loop via bpy.app.timers,
which never fires in background mode; this driver instead calls
BlenderMCPServer.execute_command() directly, which runs on the main thread
(the script's own thread), keeping every bpy access on the main thread.

Usage:
    blender -b -P blender_test_driver.py -- <path/to/addon.py> <port>
"""

import importlib.util
import json
import socket
import sys
import traceback

import bpy

def main():
    args = sys.argv
    if "--" in args:
        rest = args[args.index("--") + 1:]
    else:
        rest = []
    addon_path = rest[0] if rest else "addon/addon.py"
    port = int(rest[1]) if len(rest) > 1 else 9876
    host = "127.0.0.1"

    # Load the addon module from the source file (not installed as an addon).
    spec = importlib.util.spec_from_file_location("blendermcp_addon", addon_path)
    addon = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(addon)

    # register() sets up the scene properties the command handlers read
    # (blendermcp_use_polyhaven, blendermcp_port, ...).
    addon.register()

    # Enable the integration toggles so the full command set is available.
    scene = bpy.context.scene
    scene.blendermcp_use_polyhaven = True
    scene.blendermcp_use_sketchfab = True
    scene.blendermcp_use_hyper3d = True
    scene.blendermcp_use_hunyuan3d = True

    # Create a known object the tests can query.
    bpy.ops.mesh.primitive_cube_add(size=2.0, location=(1.0, 2.0, 3.0))

    srv = addon.BlenderMCPServer(host, port)
    srv.running = True
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind((host, port))
    sock.listen(1)
    print("DRIVER_READY", flush=True)

    while True:
        client, _ = sock.accept()
        buffer = b""
        try:
            # Mirror the addon's _handle_client: keep the connection open and
            # serve multiple commands per connection until the client
            # disconnects (recv returns b''), which is what the real MCP
            # server and the Rust BlenderConnection expect.
            while True:
                data = client.recv(8192)
                if not data:
                    break
                buffer += data
                try:
                    command = json.loads(buffer.decode("utf-8"))
                except json.JSONDecodeError:
                    # Incomplete payload; wait for more bytes.
                    continue
                buffer = b""
                response = srv.execute_command(command)
                client.sendall(json.dumps(response).encode("utf-8"))
        except Exception as exc:  # noqa: BLE001 - keep the server alive
            traceback.print_exc()
            try:
                client.sendall(
                    json.dumps({"status": "error", "message": str(exc)}).encode("utf-8")
                )
            except Exception:
                pass
        finally:
            client.close()

if __name__ == "__main__":
    main()
