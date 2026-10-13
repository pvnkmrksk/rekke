# Recording video

Press **Record**, start with `--record`, or set `record = true` for a rig in its
manifest. Recording shows the camera image as it arrived: without overlays, and
before background subtraction or inversion, so a recording can always be
re-tracked with different settings later.

## What a recording contains

Each session gets its own folder under `recording.directory` (relative to the
config; `recordings/` by default), named from the label, the start time and the
process:

```text
recordings/rig-a-1791769898523379000-63855/
  video-0000.avi, video-0001.avi …   split at segment_mb (1 GiB) to stay within AVI limits
  frames.jsonl                       arrival time and camera sequence number of every frame
  config.toml                        the settings used
  background.png                     the saved background, if any
```

`frames.jsonl` has one line per recorded frame, so every frame can be matched to
the measurements published while it was tracked (`sequence` and `timestamp_ns`
in the [measurement format](../reference/measurements.md)).

## Formats

| `format` | Size at 640×480, 60 fps | Bit-exact | Plays in |
| --- | --- | --- | --- |
| `"mjpeg"` (default) | ≈ 1.5–3 MB/s (≈ 5–10 GB per hour) | No; quality 90 is visually lossless for tracking | Rekkefly, VLC, FFmpeg |
| `"png"` | ≈ 8–10 MB/s | Yes | Rekkefly, FFmpeg |
| `"raw"` | 18.4 MB/s (66 GB per hour) | Yes | Rekkefly, VLC, FFmpeg |

Set `quality` (1–100) for MJPEG. On the Bogong moth, quality 75 tracked the same
as lossless frames. To shrink older raw recordings, use
[`rekkefly clip`](video.md#cut-and-shrink-videos-rekkefly-clip).

## Never a silently dropped frame

The camera thread only hands each frame to the recorder; encoding runs on its
own threads, in parallel when frames queue up. Up to `queue_frames` frames (32 by
default) can wait for the disk. If the disk cannot keep up and the queue fills,
recording **stops with a visible error** and tracking continues. A recording
never has frames silently missing.

Headless runs report what was written:

```sh
rekkefly --config rig.toml --headless --record --frames 600 --json
```

```json
{"frames":600, …, "recording":{"accepted":600,"directory":"…/recordings/rig-a-…","error":null,"written":600}, …}
```

## Settings

| Key | Default | Meaning |
| --- | --- | --- |
| `recording.enabled` | `false` | Record from the start of every run |
| `recording.directory` | `"recordings"` | Where session folders go, relative to the config |
| `recording.format` | `"mjpeg"` | `"mjpeg"`, `"png"` or `"raw"` |
| `recording.quality` | `90` | MJPEG quality, 1–100 |
| `recording.queue_frames` | `32` | Frames that may wait for the disk (1–256, at most 256 MiB) |
| `recording.segment_mb` | `1024` | Size at which a new `video-NNNN.avi` starts (16–1900) |

Format changes apply to the next recording. The **Record** button and `--record`
never change `recording.enabled` in the saved config.

## Playing recordings

Recordings open in Rekkefly like any video (`rekkefly recording/video-0000.avi`),
including recordings cut short by a crash or power loss. MJPEG and raw AVI also
play in VLC and FFmpeg.
