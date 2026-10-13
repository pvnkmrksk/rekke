# Cameras

Rekkefly reads USB and built-in cameras natively on macOS (AVFoundation),
Windows (Media Foundation) and Linux (Video4Linux2). It converts every frame to
8-bit grayscale at the configured processing size, `source.width × source.height`
(640×480 by default).

## Finding your cameras

```sh
rekkefly --list-cameras
```

```text
Use the number (or the path/name) as source.device:
  0: HD Pro Webcam C920 (/dev/video0)
  1: USB 2.0 Camera (/dev/video2)
```

Add `--json` for a machine-readable list. On a Mac use
`/Applications/Rekkefly.app/Contents/MacOS/rekkefly --list-cameras`.

## Choosing a camera

Set `source.device` in the rig's config, pick a camera in **Settings → Camera /
input**, or pass `--device` for one run:

| Value | Meaning | Best for |
| --- | --- | --- |
| `"auto"` | The first camera that opens. Cameras in use by another rig or program are skipped | One camera |
| `"0"`, `"1"`, … | The camera's position in `--list-cameras`, from 0 | Quick setups |
| `/dev/v4l/by-id/usb-…-video-index0` | A Linux device path. Stays the same when cameras are re-plugged in another order | Fixed Linux rigs |
| `"C920"` | Part of the camera's name | Mac and Windows rigs with different camera models |

On Linux, USB cameras also create metadata-only `/dev/video*` nodes. Rekkefly
lists only nodes that capture video, in number order, so `"1"` means the second
*camera*: often `/dev/video2`, not `/dev/video1`.

```sh
rekkefly --device 1 --label arena-2     # the second camera, for this run only
```

`--device` and the other [run-only overrides](../reference/cli.md#run-only-overrides)
are never saved to the config.

## Size and frame rate

Rekkefly asks the camera for `source.width`, `source.height` and `source.fps`.
If the camera does not offer exactly that mode, it picks the nearest one and
scales the image to the processing size **without changing its aspect ratio**:
stretching would bias every angle. Cameras that offer grayscale modes are used
in grayscale directly, which saves a conversion.

Tracking processes **every** frame the camera delivers. The GUI preview is
drawn at `preview_fps` (40 by default), independently.

Each frame carries the time it arrived and its sequence number. If the computer
falls behind, the camera driver skips exposures; Rekkefly reports such gaps
(`skipped_frames` in MCP measurements) rather than hiding them.

## Permissions

| System | If the camera does not open |
| --- | --- |
| macOS | **System Settings → Privacy & Security → Camera**: turn Rekkefly on. A terminal running `rekkefly` needs the permission itself |
| Windows | **Settings → Privacy & security → Camera**: allow desktop apps |
| Linux | Add yourself to the `video` group: `sudo usermod -aG video $USER`, then log out and back in |

## No camera

In a window opened without a camera, Rekkefly plays the
[Bogong moth example](../getting-started/bogong-moth.md) with an amber banner,
so you can work on the markers and settings. Click **Retry camera** after
connecting one. Headless runs and rigs started from a manifest stop with an
error instead.

## Network streams

`source.kind = "network"` reads an RTSP, HTTP or other stream through FFmpeg:

```toml
[source]
kind = "network"
url = "rtsp://192.168.1.20:8554/fly"
width = 640
height = 480
fps = 30
```

FFmpeg must be installed (see [Install](../getting-started/install.md#optional-ffmpeg)).

## The simulated fly

`source.kind = "demo"`, or `--demo` for one run, generates a backlit tethered fly
with beating wings, legs and antennae, so you can test outputs and rigs with no
camera. `beat_hz` sets how fast its stroke amplitude changes (below half the
frame rate).
