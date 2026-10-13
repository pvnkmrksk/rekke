# Configuration

Every rig has one TOML file: its source, markers, detector settings, outputs and
recording. The GUI saves edits back into it automatically, keeping its comments.
Configs are validated when loaded; an invalid value is reported with the key's
name, and nothing is changed.

Relative paths in a config are relative to the config file. Edits that change the
source, the outputs or the label take effect after **Settings → Restart
acquisition**. Everything else applies to the next frame.

`tests/docs.rs` checks that every key of the default config appears on this page.

## Top level

| Key | Default | Meaning |
| --- | --- | --- |
| `instance` | `"rig-a"` | Label for the window title, logs and recordings (1–128 characters) |
| `autosave` | `true` | Save GUI edits automatically, 300 ms after the last change and on close |
| `preview_fps` | `40` | GUI refresh rate, 1–120. Tracking always uses every frame |

## `[source]`

| Key | Default | Meaning |
| --- | --- | --- |
| `source.kind` | `"camera"` | `"camera"`, `"video"`, `"network"` (through FFmpeg) or `"demo"` (simulated fly) |
| `source.device` | `"auto"` | Camera: `"auto"`, a number from `--list-cameras`, a device path, or part of the name ([choosing a camera](../guide/cameras.md#choosing-a-camera)) |
| `source.path` | | Video file (`kind = "video"`) |
| `source.repeat` | `false` | Video: loop at the end |
| `source.realtime` | `true` | Video: play at the recorded frame rate; `false` processes as fast as possible |
| `source.url` | | Stream URL (`kind = "network"`), for example `rtsp://…` |
| `source.beat_hz` | `0.8` | Simulated fly: how fast its stroke changes, below `fps / 2` |
| `source.width`, `source.height` | `640`, `480` | Processing size, 32–8192 each, at most 32 megapixels. Other sizes are scaled to fit without distortion |
| `source.fps` | `30` | Requested camera rate, 1–10000 |

Only the keys of the chosen `kind` are allowed. A config with `kind = "camera"`
and a `path` is an error, so a typo cannot be silently ignored.

## Body axis: `[head]`, `[tail]`

| Key | Default | Meaning |
| --- | --- | --- |
| `head.x`, `head.y` | `320`, `175` | The head end of the body axis (pixels) |
| `tail.x`, `tail.y` | `320`, `330` | The tail end of the body axis (pixels) |

The head and tail must be distinct, and the body axis must not be parallel to
the line between the wing hinges.

## `[tracking]`

| Key | Default | Meaning |
| --- | --- | --- |
| `tracking.asymmetric` | `false` | Edit the wings independently instead of mirroring |
| `tracking.dark_on_light` | `true` | A dark insect on a light background. `false` for a bright insect on dark |
| `tracking.subtract_background` | `false` | Track the difference from the saved background |
| `tracking.background_path` | `"rig-a.background.png"` | The background image, saved by **Save BG** |
| `tracking.angle_bins` | `240` | Angular samples per sector, 16–2048 |
| `tracking.radial_samples` | `72` | Radial samples per sector, 4–1024 (bins × samples at most 524,288) |

## Parts: `[tracking.left]`, `[tracking.right]`, `[tracking.head]`, `[tracking.abdomen]`

Each part has the same keys. Here `<part>` stands for `left`, `right`, `head` or `abdomen`.

| Key | Default (wings) | Meaning |
| --- | --- | --- |
| `tracking.<part>.enabled` | `true` (`false` for head and abdomen) | Track this part |
| `tracking.<part>.hinge.x`, `tracking.<part>.hinge.y` | left (296, 240), right (344, 240) | The hinge (pixels): where the wing turns, at its base. For the head and abdomen, the pivot |
| `tracking.<part>.start_deg` | left `135`, right `-85` | Where the sector starts: image degrees, clockwise from image-right |
| `tracking.<part>.span_deg` | `130` | The sector's angular size, 5–355 |
| `tracking.<part>.inner_radius` | `55` | Inner radius (pixels): just past the body |
| `tracking.<part>.outer_radius` | `185` | Outer radius (pixels): inside the wing blur |
| `tracking.<part>.method` | `"wing_blur"` (wings), `"centroid"` (head, abdomen) | `"wing_blur"` robust, `"edge"` classic, `"centroid"`. `"robust"` is accepted for `"wing_blur"` |
| `tracking.<part>.primary_edge` | `"strongest"` | Which edge is the angle: `"strongest"`, `"toward_head"` (leading edge) or `"toward_tail"` (trailing edge) |
| `tracking.<part>.threshold` | `0.0` | Minimum edge strength, 0–1. 0 reports the strongest edge |
| `tracking.<part>.max_edges` | `2` | Edges reported in `angles_rad`, 1–8 |
| `tracking.<part>.spike_width_deg` | `5.0` | Robust detector: features narrower than this are ignored, 0–45 |
| `tracking.<part>.contrast_cap` | `0.12` | Robust detector: contrast limit above the background, as a fraction of full brightness |

What these mean, and when to change them: [The detectors](../tracking/detectors.md)
and [Tuning a rig](../tracking/tuning.md).

## `[output]`

| Key | Default | Meaning |
| --- | --- | --- |
| `output.zmq_enabled` | `true` | Publish Kinefly-compatible ZMQ |
| `output.zmq_bind` | `"tcp://*:9871"` | ZMQ publisher address. `tcp://127.0.0.1:9871` keeps it on this computer |
| `output.enabled` | `true` | Send full measurements over UDP |
| `output.bind` | `"127.0.0.1:5501"` | This rig's own UDP sending address; every rig needs a different port |
| `output.target` | `"127.0.0.1:5601"` | Where your UDP receiver listens |
| `output.jsonl_path` | `""` | Also write every measurement to this JSONL file. Empty = off. Never overwritten |

See [Receiving measurements](../guide/outputs.md).

## `[recording]`

| Key | Default | Meaning |
| --- | --- | --- |
| `recording.enabled` | `false` | Record from the start of every run |
| `recording.directory` | `"recordings"` | Where recording sessions go |
| `recording.format` | `"mjpeg"` | `"mjpeg"`, `"png"` or `"raw"` |
| `recording.quality` | `90` | MJPEG quality, 1–100 |
| `recording.queue_frames` | `32` | Frames that may wait for the disk, 1–256 (at most 256 MiB) |
| `recording.segment_mb` | `1024` | Start a new AVI file at this size, 16–1900 |

See [Recording video](../guide/recording.md).

## A complete config

The config for the first camera, as shipped, with its comments:

```toml
{{#include ../../../configs/camera-a.toml}}
```

The other shipped configs are in
[`configs/`](https://github.com/pvnkmrksk/rekkefly/tree/main/configs):
`camera-b.toml` (second camera and ports), `bogong-moth.toml` (the example video),
`demo.toml` (the simulated fly), `network.toml` (an RTSP stream) and
`rigs.toml` (a rig manifest).

## Rig manifests

`rigs.toml` lists rigs to start together; its keys are in
[Several rigs at once](../guide/rigs.md#manifest-keys).
