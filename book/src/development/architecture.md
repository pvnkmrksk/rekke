# Architecture

Rekkefly is one Rust crate: a library (`src/lib.rs`) with everything that
tracks, records and publishes, and a binary (`src/main.rs`) for the command
line. The GUI and the MCP server are clients of the same library.

## A frame's journey

```text
 camera / video / stream / simulated fly
        │  source.rs, camera.rs, avi.rs
        ▼
 capture thread (one per rig) ── engine.rs
        │  8-bit gray frame, sequence number, arrival time
        ├──► tracker.rs: sample each sector, find edges ──► Measurement
        │        │
        │        ├──► ZMQ publisher (zmq_output.rs, a thread per subscriber)
        │        ├──► UDP sender (output.rs)
        │        ├──► JSONL file (measurement_log.rs)
        │        └──► subscribers: GUI plots, MCP `measure`
        │
        ├──► recorder (recording.rs): encoder threads, AVI segments, frames.jsonl
        └──► latest frame for the preview (GUI, MCP `snapshot`)
```

Tracking runs on the capture thread, so there is no queue between a frame
arriving and its measurement being published. Everything slower than tracking,
such as encoding, drawing and disk writes, runs on other threads and receives
frames by reference, never by copy. Outputs never block tracking: a full
subscriber queue drops messages for that subscriber, and the recorder stops
loudly rather than silently dropping frames.

## Modules

| File | Role |
| --- | --- |
| `src/camera.rs` | Camera listing and selection, mode negotiation, fast grayscale paths |
| `src/source.rs` | Image sources: camera, native AVI, FFmpeg, simulated fly, the bundled example |
| `src/avi.rs` | AVI reader and writer (MJPEG / PNG / Y800), including OpenDML and interrupted files |
| `src/frame.rs` | Grayscale frames and scaling without distortion |
| `src/tracker.rs` | Polar sampling and the classic, robust and centroid detectors; the measurement schema |
| `src/synth.rs` | The synthetic tethered fly with ground truth |
| `src/engine.rs` | The acquisition thread, commands, fallback to the example, measurement subscriptions |
| `src/analyze.rs`, `src/overlay.rs` | Offline tracking and summaries; drawing the tracking onto frames |
| `src/mcp.rs` | The Model Context Protocol server |
| `src/recording.rs`, `src/clip.rs` | Video recording; cutting and converting videos |
| `src/output.rs`, `src/zmq_output.rs`, `src/measurement_log.rs` | UDP, ZMQ and JSONL outputs |
| `src/rigs.rs`, `src/lock.rs`, `src/installation.rs` | Rig manifests, one-window-per-config locks, per-user configs |
| `src/config.rs`, `src/geometry.rs`, `src/persistence.rs` | Settings and validation, marker editing, atomic autosave |
| `src/gui.rs` | The desktop window |

## Design rules

- **Never regress tracking.** Detector changes are judged by the synthetic and
  real-footage tables ([Tests](testing.md)).
- **No work on the capture thread that can wait.** No allocations or frame
  copies in the tracking path; sampling positions are precomputed.
- **Never lose user data.** Config writes are atomic and keep comments.
  Recordings never drop frames silently. Run-only overrides are never saved.
- **Fail loudly in experiments, gently in the GUI.** A missing camera or busy
  port is an error headless and in rigs; in a double-clicked window it falls back
  with a visible banner.
- **Every operation is scriptable.** Each GUI action has a command-line or MCP
  equivalent with machine-readable output.

## Native window lifecycle

The eframe/egui 0.36 integration polls worker results and shutdown in
`App::logic`, including while the window is hidden. `App::ui` builds the panels
and camera view from the root UI. Tracking stays on the capture worker, while
preview textures are refreshed when the UI is drawn.

## Dependencies

| Purpose | Crates |
| --- | --- |
| Window | `eframe` / `egui` |
| Cameras | `nokhwa` (AVFoundation, Media Foundation), `v4l` (Linux) |
| Outputs | None: the ZMQ publisher (`zmq_output.rs`) speaks ZMTP 2.0 and 3.0 itself, as libzmq does |
| Images and video | `zune-jpeg` (decoding), `jpeg-encoder` (encoding), `image` (PNG) |
| Configs | `serde`, `toml`, `toml_edit` (comment-preserving writes) |
| Command line | `clap` |

There is no C++ computer-vision library, no Python and no ROS: everything
specific to wing tracking is in `src/tracker.rs`.
