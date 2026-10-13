# Driving Rekkefly from AI agents and scripts (MCP)

`rekkefly mcp` is a [Model Context Protocol](https://modelcontextprotocol.io)
server. Any MCP client (Claude Code, Claude Desktop, Cursor, VS Code and others)
can then run your rigs. It can find cameras, edit rig configs, start and stop
tracking, read live wing angles, measure a trial and look at the tracking
overlay, all without a window. It speaks newline-delimited JSON-RPC 2.0 on
stdin and stdout (the MCP stdio transport) and accepts protocol versions
2024-11-05 through 2025-11-25.

## Connect a client

The server is the Rekkefly program itself with the argument `mcp`.

| Installed as | Command |
| --- | --- |
| Mac app | `/Applications/Rekkefly.app/Contents/MacOS/rekkefly` |
| Windows installer | `%LOCALAPPDATA%\Programs\Rekkefly\rekkefly.exe` |
| Ubuntu `.deb` | `rekkefly` |
| Portable folder | the `rekkefly` (or `rekkefly.exe`) inside it |

**Claude Code**

```sh
claude mcp add rekkefly -- rekkefly mcp
# Mac app:
claude mcp add rekkefly -- /Applications/Rekkefly.app/Contents/MacOS/rekkefly mcp
```

This repository also has a project `.mcp.json`. Claude Code opened in a clone
offers a `rekkefly` server built from source that uses `configs/`.

**Claude Desktop, Cursor and other clients** take the same command in their
JSON settings, for example Claude Desktop's `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "rekkefly": {
      "command": "/Applications/Rekkefly.app/Contents/MacOS/rekkefly",
      "args": ["mcp"]
    }
  }
}
```

Config names without a folder (`camera-a`, `bogong-moth`) are looked up in the
folder Rekkefly itself uses: the user configs of an installed app, a portable
folder's `configs/`, or `configs/` in a clone. Pass `--config-dir DIR` to use
another folder. Rigs started over MCP hold the same per-config lock as windows,
so an agent cannot open a rig that is already open in the GUI, and the reverse.

## Tools

| Tool | What it does | Changes anything |
| --- | --- | --- |
| `list_cameras` | Cameras and the numbers to use as `device` | No |
| `list_configs` | Every rig config with its source, outputs and whether it runs; rig manifests | No |
| `get_config` | A config as JSON and as its commented TOML | No |
| `update_config` | Change fields with a JSON merge patch, validated, saved with comments kept; applied live to a running rig | Writes the config |
| `start_rig` | Track headless with a config; run-only `device`, `video`, `demo`, `label`, ports, `record` | Opens the camera, binds ports |
| `launch_rigs` | Every rig in `rigs.toml`, all or none | Same, per rig |
| `rig_status` | Frames, rate, tracking time, latest angles, recording, outputs | No |
| `measure` | Every frame for N seconds: mean, spread and range of each angle; optional CSV | Writes the CSV if asked |
| `snapshot` | A JPEG of the frame with the tracking drawn on it, plus its angles (live rig or any video frame) | No |
| `record` | Start or stop recording | Writes video |
| `save_background` | Save the next frame as the background | Writes the background PNG |
| `stop_rig` | Stop one rig or all, finishing recordings | |
| `track_video` | Track a whole video as fast as possible; summary, optional CSV/JSONL | Writes CSV/JSONL if asked |
| `clip_video` | Cut a video into a smaller AVI (never overwrites) | Writes the new AVI |
| `benchmark` | Tracking time per frame for a config | No |

`tools/list` gives each tool's full JSON Schema. Unknown argument names are
rejected with the list of valid ones, so a typo never silently does something else.
A tool that fails returns `isError: true` with a plain-English reason the agent
can act on, for example "No rig "arena-2" is running here. Running: arena-1".

## Conventions

- **Angles** are in degrees in tool results (radians on ZMQ and UDP). Wing
  angles are 0 perpendicular to the body axis and positive toward the head;
  `left_minus_right` is L − R.
- **Geometry** is in image pixels. Sector `start_deg` and `span_deg` are image
  degrees, clockwise from image-right. `snapshot` shows the result; check it
  after every marker change.
- **Run-only overrides** (`device`, `video`, `demo`, `label`, ports, `record` in
  `start_rig`) are never saved to the config, exactly as on the command line.

## An example session

An agent asked to "start arena 1 on camera 1, check the markers, and give me the
mean L − R over a 10-second trial" might call (outputs shortened and illustrative):

```text
list_cameras                       → 0: Integrated Camera, 1: DMK 33UX… 
start_rig  {config: "camera-a", device: "1", label: "arena-1"}
snapshot   {rig: "arena-1"}        → image: hinges sit on the wing bases?
update_config {config: "camera-a", changes: {tracking: {left: {hinge: {x: 296, y: 238}}}}}
snapshot   {rig: "arena-1"}        → check again
measure    {rig: "arena-1", seconds: 10, csv: "trial-01.csv"}
           → frames 1200, left_minus_right mean −3.2°, sd 4.1°, …
stop_rig   {rig: "arena-1"}
```

Offline, with no camera:

```text
track_video {video: "samples/bogong-moth.avi", csv: "moth.csv"}
snapshot    {video: "samples/bogong-moth.avi", frame: 120}
```

## Without MCP

The same operations are available to scripts:

```sh
rekkefly --list-cameras --json
rekkefly track recording.avi --csv angles.csv --json     # summary as JSON
rekkefly --config rig.toml --headless --frames 600 --json
rekkefly --benchmark --json
```

Live measurements stream over ZMQ (Kinefly format) and UDP (full JSON) while
any rig runs; see [Receiving measurements](https://pvnkmrksk.github.io/rekkefly/docs/guide/outputs.html).
