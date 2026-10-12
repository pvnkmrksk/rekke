<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/banner-dark.png">
  <img src="docs/images/banner-light.png" alt="Rekkefly: the new Kinefly. Real-time wing tracking for tethered insects on Mac, Windows and Linux." width="760">
</picture>

[![Latest release](https://img.shields.io/github/v/release/pvnkmrksk/rekke?label=download&color=ff7059)](https://github.com/pvnkmrksk/rekke/releases/latest)
[![Build and release](https://github.com/pvnkmrksk/rekke/actions/workflows/build.yml/badge.svg)](https://github.com/pvnkmrksk/rekke/actions/workflows/build.yml)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-1f5c66)
[![Kinefly compatible](https://img.shields.io/badge/ZMQ-Kinefly%20compatible-122329)](#kinefly-compatible-zmq)
[![License: MIT](https://img.shields.io/badge/license-MIT-5b6b6e)](LICENSE)

**Rekkefly is the new Kinefly.** It tracks the wings of a tethered insect (*Drosophila* and other flies, bees, moths) from a camera or a video file. It streams the wing angles and **L − R** (left minus right wingbeat amplitude) to your closed-loop experiment in real time. It is one native app: download, double-click and track. There is no ROS, Python, OpenCV or Docker to install.

The name joins **ರೆಕ್ಕೆ** (*rekke*, the [Kannada word for wing](https://alar.ink/dictionary/kannada/english/%E0%B2%B0%E0%B3%86%E0%B2%95%E0%B3%8D%E0%B2%95%E0%B3%86)) with **fly**, and nods to its roots in [Kinefly](https://github.com/ssafarik/Kinefly).

> **Looking for Kinefly?** Rekkefly does the same job: wing-stroke-amplitude tracking from annular sectors around the wing hinges. It speaks the same ZMQ message as [Kinefly Docker](https://github.com/pvnkmrksk/Kinefly_Docker), so your existing subscribers keep working. See [From Kinefly to Rekkefly](#from-kinefly-to-rekkefly).

![Rekkefly tracking both wings of a tethered fly](docs/images/main-window.png)

- **Download, double-click, track.** Your camera opens. With no camera, a 30-second sample video plays so you can learn the app.
- **No L − R spikes from legs or antennae.** The default detector follows the broad motion-blur shoulder of the wing. Narrow features (legs, antennae, bristles), even ones as dark as the wing, can't pull the angle, whether they lie inside or outside the blur ([details](#ignore-legs-and-antennae)).
- **Any number of rigs.** Each rig has its own camera, ports, label and settings. Start them all with one command from one TOML file.
- **Cameras or video files.** Drop a video on the window, or analyse recordings offline as fast as your computer allows.
- **Small recordings.** MJPEG by default, about 10× smaller than raw. Lossless PNG and raw are one setting away.
- **Kinefly-compatible output.** ZMQ JSON on port 9871 works with existing Kinefly Docker subscribers. Full measurements are available over UDP or JSONL.

---

## Contents

1. [Quick start](#quick-start)
2. [From Kinefly to Rekkefly](#from-kinefly-to-rekkefly)
3. [Using the app](#using-the-app)
4. [Video files](#video-files)
5. [Several rigs at once](#several-rigs-at-once)
6. [Ignore legs and antennae](#ignore-legs-and-antennae)
7. [Recording video](#recording-video)
8. [Receiving measurements](#receiving-measurements)
9. [Configuration reference](#configuration-reference)
10. [Command-line reference](#command-line-reference)
11. [Troubleshooting](#troubleshooting)
12. [Installing: details per system](#installing-details-per-system)
13. [Building, testing and benchmarks](#building-testing-and-benchmarks)

---

## Quick start

**1. Download** the installer for your computer from [Releases](https://github.com/pvnkmrksk/rekke/releases/latest):

| Computer | Download | Install |
| --- | --- | --- |
| Mac, Apple silicon (M1 and later) | `Rekkefly-0.6.0-macos-arm64.dmg` | Open it and drag **Rekkefly** onto **Applications** |
| Mac, Intel | `Rekkefly-0.6.0-macos-x64.dmg` | Open it and drag **Rekkefly** onto **Applications** |
| Windows 10/11 | `Rekkefly-0.6.0-windows-x64-setup.exe` | Run it. No administrator rights needed |
| Ubuntu 22.04 or newer | `rekkefly_0.6.0_amd64.deb` | `sudo apt install ./rekkefly_0.6.0_amd64.deb` |

On a Mac, the first launch may be blocked because the app is not notarized. See [Mac: first launch](#mac-first-launch).

**2. Open Rekkefly.** It uses the first camera it can open. Allow camera access if your system asks. With no camera, the bundled sample video plays and an amber banner says so. Click **Retry camera** after connecting one.

**3. Place the markers** by dragging the dots:

| Dot | Put it on |
| --- | --- |
| Magenta / cyan | The head and the tail. Together they define the body axis |
| Red / green (inner) | The left and right wing hinges |
| Corner dots | The sector's angular limits and outer radius, in one drag |
| Middle dots on an arc | Only the inner or outer radius |
| White centre dot | Moves the whole setup |

The two wings mirror each other while you edit. Tick **Asymmetric** to edit them separately.

**4. Read the angles.** The lower-left box shows the left and right wing angles and L−R in radians. The same values are published continuously on **ZMQ port 9871** and over UDP ([how to receive them](#receiving-measurements)).

Every change saves automatically to the rig's config file. Close the window and reopen it: everything is where you left it, including the saved background.

---

## From Kinefly to Rekkefly

Rekkefly stands on the shoulders of two projects.

**[Kinefly](https://github.com/ssafarik/Kinefly)** made real-time wing tracking practical for tethered-flight rigs. It measures the leading edge of each wing's motion-blur stroke envelope inside annular sectors around the wing hinges, together with head and abdomen angles, and publishes the results for closed-loop experiments. Many labs built their arenas around it. Kinefly was written for ROS 1 and Python 2, both now end-of-life, and it is no longer maintained. Installing it today means fighting old operating systems and dependency chains (the [2015 installation notes](https://peterthomasweir.blogspot.com/2015/07/installation-notes-for-kinefly.html) give a flavour), and the old stack can be fragile in long experiments.

**[Kinefly Docker](https://github.com/pvnkmrksk/Kinefly_Docker)** kept Kinefly alive. It packed the original ROS stack into a container and added `ros_zmq_bridge.py`, which re-publishes the wing angles as simple JSON over ZMQ, so any language could listen without ROS. Experiments ran on it; its message format is the contract Rekkefly keeps.

**Rekkefly** is the next step: a from-scratch native app that keeps what worked and drops the stack.

| | Kinefly | Kinefly Docker | Rekkefly |
| --- | --- | --- | --- |
| Install | ROS 1 + Python 2 + OpenCV on an old Ubuntu | Docker, X forwarding, camera pass-through | One installer or app (Mac, Windows, Linux) |
| Cameras | ROS camera drivers | ROS camera drivers in the container | Native USB/webcam, video files, network streams |
| Output | ROS topics | ZMQ JSON via the bridge | **The same ZMQ JSON**, plus full UDP/JSONL measurements |
| Legs and antennae in the wing region | Can pull the edge | Can pull the edge | Shoulder detector ignores narrow features |
| Several rigs | One ROS launch per rig | One container per rig | One TOML, one command, one window per rig |
| Recording | ROS bags | ROS bags | MJPEG/PNG/raw AVI with exact frame times |
| Maintained | No | Bridge only | Yes |

Rekkefly keeps Kinefly's geometry and conventions: annular sectors, angles measured from the outward perpendicular to the body axis, positive toward the head. Settings from Kinefly can be re-created by dragging the sectors onto the fly. The ZMQ message has the bridge's exact fields and values: `{"x": left, "y": right, "yaw": left − right, …}`. **Point your existing subscriber at Rekkefly and it keeps working.** Rekkefly was written from scratch after studying Kinefly's documentation and `tracker_edge.py`; it shares no code with it.

---

## Using the app

![Settings and signal diagnostics](docs/images/settings.png)

**Toolbar**

| Control | What it does |
| --- | --- |
| **Save BG** | Saves the next frame as the background image. Capture an *empty* scene first |
| **H A L R** | Turns tracking of the head, abdomen, left and right wing on or off |
| **Asymmetric** | Edits each wing on its own instead of mirroring |
| **Subtract** | Tracks the difference from the saved background |
| **Invert** | For a bright insect on a dark background |
| **Threshold L / R** | 0 reports the strongest edge, however faint. Raise it to reject noise. Applies live |
| **Ignore legs** | Robust wing detector, on by default ([why](#ignore-legs-and-antennae)) |
| **Record** | Starts and stops video recording ([formats](#recording-video)) |
| **New window** | Opens another Rekkefly window for the next rig, with its own camera and ports |
| **Settings** | Detector, precise geometry, camera, outputs, recording and sampling |

**Angles.** Wing angles are 0 when the wing points straight out from the body axis and positive toward the head. Kinefly uses the same convention. Head and abdomen angles follow the body axis, with clockwise positive in the image. All transmitted angles are in radians; geometry settings are in degrees.

**Diagnostics.** In **Settings**, tick **Show signal diagnostics**. You get an 8-second plot of both wing angles and the selected sector's angular intensity profile. The profile is auto-scaled, and detected edges are marked with vertical lines (the brighter line is the reported one).

**Saving.** Edits are written 300 ms after you stop dragging, and when you close the window. Writes are atomic, so a crash cannot corrupt the file, and comments in the file are kept. Only one window may use a config file at a time. If a second window is started without `--config`, it takes the next rig's config instead.

---

## Video files

**Open a video:** drag it onto the window, or run:

```sh
rekkefly recording.avi            # play at the recorded speed
rekkefly recording.avi --loop     # loop it
```

AVI files play natively, with no extra software. This covers Rekkefly and Wingtrace recordings (MJPEG, PNG, raw Y800) and most uncompressed 8/24/32-bit AVIs. Recordings cut short by a crash or power loss also play. Other formats such as MP4 and MOV need [FFmpeg](https://ffmpeg.org/download.html) installed. Videos of a different size are scaled to the configured size **without changing their aspect ratio**, because stretching the image would bias every angle.

A video opened like this applies **to this run only**. Your saved camera setup is untouched. **Settings → Camera / input** has **Back to saved source** and **Save as this config's source**.

**Offline analysis** processes every frame as fast as possible and writes one JSON line per frame:

```sh
rekkefly --config my-rig.toml recording.avi --fast --headless
```

Set `output.jsonl_path = "results.jsonl"` in `my-rig.toml` to choose the output file. At 640×480 this runs at several hundred frames per second, including MJPEG decoding (about 450 frames/s on the 4-core test machine).

**Cut a snippet, or shrink a raw recording**, with `rekkefly clip`. It never overwrites a file:

```sh
# 30 seconds starting 1 minute in, as small MJPEG
rekkefly clip video-0003.avi snippet.avi --start 60 --duration 30
# Convert a whole raw recording to lossless PNG frames (about half the size)
rekkefly clip video-0000.avi video-0000-png.avi --format png
# Keep every 2nd frame
rekkefly clip big.avi half-rate.avi --every 2
```

### The sample video

`samples/sample-fly.avi` is the 30-second clip that plays when no camera is available. It ships with every package. To use **your own footage** as the sample, see [samples/README.md](samples/README.md). It takes one command.

---

## Several rigs at once

Each rig needs its own **camera**, its own **ports** and its own **config file**, which holds its markers, background and recordings. Rekkefly checks all three before starting anything, so two rigs can never fight over a camera or a port.

### Quick: one more window

Click **New window**, or start Rekkefly a second time without arguments. The new window takes the next rig config: `camera-b.toml` uses camera 1 and ports 5502/9872. A third window creates `camera-c.toml` with camera 2 and ports 5503/9873, and so on.

### Repeatable: every rig with one command

Describe your rigs once in `rigs.toml`:

```toml
[[rigs]]
label = "left-arena"       # window title, logs, recording folders
device = "0"               # first camera in `rekkefly --list-cameras`
zmq_port = 9871
udp_port = 5501
udp_target = "127.0.0.1:5601"

[[rigs]]
label = "right-arena"
device = "/dev/v4l/by-id/usb-046d_HD_Pro_Webcam_C920_ABC123-video-index0"
zmq_port = 9872
udp_port = 5502
udp_target = "127.0.0.1:5602"
record = true              # this rig records from the start
```

Then start them all:

```sh
rekkefly --rigs rigs.toml                     # one window per rig
rekkefly --rigs rigs.toml --headless --record # no windows, record everything
rekkefly --rigs rigs.toml --demo              # dry run with simulated flies
```

```text
Started left-arena   PID 4121    camera 0 · ZMQ tcp://*:9871 · UDP 127.0.0.1:5501 → 127.0.0.1:5601 [.../left-arena.toml]
Started right-arena  PID 4122    camera /dev/v4l/by-id/... · ZMQ tcp://*:9872 · UDP 127.0.0.1:5502 → 127.0.0.1:5602 [.../right-arena.toml]
Press Ctrl+C to stop every rig.
```

| Key | Meaning |
| --- | --- |
| `label` | Name in the window title, logs and recording folders |
| `config` | The rig's own settings file. Default `<label>.toml` beside `rigs.toml`. It is **created on first launch** if missing |
| `device` | Camera: `"auto"`, a number, a device path, or part of the camera's name (see below) |
| `video` | Track a video file instead of a camera (path relative to `rigs.toml`) |
| `zmq_port` | Kinefly-format ZMQ publisher port |
| `udp_port` / `udp_target` | Local sending port and `IP:port` destination for full UDP measurements |
| `record` | Start recording when the rig starts |
| `enabled` | `false` skips the rig |

Values in `rigs.toml` apply to that launch. Everything else, such as markers, detector settings and the background, lives in the rig's config file and is saved there as you edit. **Ctrl+C** stops all rigs and finishes every recording cleanly. If one rig fails, the others are stopped and the error is printed.

Installed apps keep a ready-made `rigs.toml` next to your configs, so plain `rekkefly --rigs` works:

| System | Your configs | All-rigs shortcut |
| --- | --- | --- |
| Mac | `~/Library/Application Support/Rekkefly/configs/` | `/Applications/Rekkefly.app/Contents/MacOS/rekkefly --rigs` |
| Windows | `%APPDATA%\Rekkefly\configs\` | Start menu → **Rekkefly - start all rigs** |
| Ubuntu | `~/.config/rekkefly/configs/` | App menu → Rekkefly → **Start all rigs** |

### Choosing cameras

```sh
rekkefly --list-cameras
```

```text
Use the number (or the path/name) as source.device:
  0: HD Pro Webcam C920 (/dev/video0)
  1: USB 2.0 Camera (/dev/video2)
```

- **A number** is the camera's position in this list, starting at 0. On Linux, metadata-only device nodes are skipped. In the example above, `"1"` is the camera at `/dev/video2`.
- **`"auto"`** uses the first camera that opens. Cameras already used by another rig or program are skipped.
- **A path** such as `/dev/v4l/by-id/...` (Linux) stays the same even if cameras are re-plugged in another order. Use it for fixed rigs.
- **Part of the name**, for example `"C920"`, also works. On Mac and Windows this is the most stable choice.

---

## Ignore legs and antennae

A beating wing photographed over many wingbeats is a faint, translucent **stroke envelope**: a broad *shoulder* of blur tens of degrees wide. Its leading edge is what wingbeat amplitude means. A leg, an antenna, a bristle or the arista is **narrow**. It is localized in angle and usually in radius, and it can sit inside the blur or outside it. It need not be darker than the wing: because it is in focus, its edges are *sharper* than the wing's soft edge. The classic strongest-edge method (Kinefly's approach) therefore jumps onto it, and **L − R spikes**.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/lr-trace-dark.png">
  <img src="docs/images/lr-trace-light.png" alt="L minus R over 12 seconds: the classic detector spikes to plus or minus 100 degrees and is biased by a line beside one wing; Rekkefly follows the ground truth within 1.2 degrees.">
</picture>

Rekkefly's default detector (TOML `method = "wing_blur"`) tracks the shoulder rather than the sharpest edge:

1. **Bands.** The sector is split into up to 8 radial bands, each giving an angular intensity profile. Contrast is capped so an opaque leg cannot outweigh a translucent wing.
2. **Narrow features are removed.** A morphological opening deletes anything narrower than `spike_width_deg` (default 5°), in each band, however dark it is. A broad shoulder passes through unchanged, edges included.
3. **Median across bands.** Something present at only some radii, as a leg or antenna usually is, is out-voted.
4. **The shoulder decides.** Each remaining edge is scored by its steepness *times the blur shoulder behind it*, averaged over a wide window (about 20°). A narrow line has almost no shoulder, even when it touches or crosses the wing, and the wing's true edge has the full width of the envelope. The angle itself still comes from the steepest point of the winning edge, so precision is unchanged.

![Classic detector pulled onto the legs (left) vs. Rekkefly (right)](docs/images/legs-classic-vs-robust.png)

*Front legs reaching forward. White dashes: the true leading edge. Orange: the detected edge. Classic detector (left) vs. Rekkefly (right).*

Measured on synthetic flies with exactly known wing angles (`cargo test --release --test intrusions -- --ignored --nocapture`):

| Scene, 640×480 (wing measurements) | Classic: off by > 4° | Rekkefly: off by > 4° | Classic: worst L − R error | Rekkefly: worst L − R error |
| --- | --- | --- | --- | --- |
| Clean flight | 0 / 720 | 0 | 0.6° | 0.6° |
| Legs kicking through the wings | 108 / 720 | 0 | 107° | 0.6° |
| Front legs reaching forward | 720 / 720 | 0 | 116° | 0.6° |
| Six narrow lines **as dark as the wing blur**, drifting in and out of it | 0 / 600 | 0 | 1.3° | 1.2° |
| Six narrow lines **twice as dark** | 51 / 600 | 0 | 73° | 1.5° |
| A line hugging one wing's leading edge, 3° outside | 219 / 600 | 0 | 4.6° (biased in 175 of 300 frames) | 0.8° |
| A line 4° *inside* the blur | 223 / 600 | 0 | 5.6° | 0.6° |
| Faint wings (7% contrast), 10 lines and kicking legs | 90 / 600 | 0 | 101° | 1.5° |

The results hold at 320×240 and 1280×960, with or without background subtraction, and for legs up to 7× thicker. The median angle error stays near 0.25°. Fast versions of these checks run as regression tests on every build.

**Limits.** These are synthetic scenes. Features **wider** than `spike_width_deg`, such as a badly out-of-focus leg 10 px wide, can still win now and then. The shoulder rule makes such cases far rarer and smaller (worst error 79° → 34° in the hardest synthetic case), but not impossible. Raise **Ignore features narrower than** in Settings for very thick legs, keeping it narrower than the wing's blur. A feature that touches the leading edge along its whole length is indistinguishable from the wing itself. Check new setups with **Show signal diagnostics**. To return to the classic detector, untick **Ignore legs** or set `method = "edge"`.

---

## Recording video

Press **Record**, start with `--record`, or set `record = true` for a rig. Each session gets its own folder under `recording.directory`:

```text
recordings/rig-a-1791769898523379000-63855/
  video-0000.avi, video-0001.avi …   split at segment_mb (1 GiB) to stay within AVI limits
  frames.jsonl                       exact receive time and camera sequence number of every frame
  config.toml                        the settings used
  background.png                     the saved background, if any
```

| `format` | Size at 640×480, 60 fps | Bit-exact | Plays in |
| --- | --- | --- | --- |
| `"mjpeg"` (default) | ≈ 1.5–3 MB/s (≈ 5–10 GB/hour) | No; quality 90 is visually lossless for tracking | Rekkefly, VLC, FFmpeg |
| `"png"` | ≈ 8–10 MB/s | Yes | Rekkefly, FFmpeg |
| `"raw"` | 18.4 MB/s (66 GB/hour) | Yes | Rekkefly, VLC, FFmpeg |

Encoding runs on its own threads, in parallel only when frames queue up. The camera thread only hands over a reference to the frame. Rekkefly never silently drops a frame. If the disk cannot keep up, recording stops with a visible error and tracking continues. Recordings show the camera image without overlays, before subtraction or inversion. Set `quality` (1–100) for MJPEG. Shrink older raw recordings afterwards with [`rekkefly clip`](#video-files).

---

## Receiving measurements

### Kinefly-compatible ZMQ

Each rig binds a ZMQ PUB socket (default `tcp://*:9871`, rig B `9872`). Subscribe to `b""` and receive **one UTF-8 JSON frame** per camera frame, exactly as the [Kinefly Docker bridge](https://github.com/pvnkmrksk/Kinefly_Docker/blob/main/ros_zmq_bridge.py) sends it:

```json
{"x":0.25,"y":-0.5,"z":0.0,"yaw":0.75,"pitch":0.0,"roll":0.0,"frame_number":77,"timestamp_unix":1700000000.125}
```

`x`/`y` are the left/right wing angles in radians, `yaw = x − y`, missing angles are `0.0`, `frame_number` is the source frame sequence and `timestamp_unix` is when the frame arrived. Rekkefly replaces the bridge, so don't run both on the same port. ZMQ PUB/SUB drops messages sent before a subscriber connects, and it never blocks tracking. A subscriber that stalls for 500 ms stops the run with an error.

```sh
python3 -m pip install pyzmq
python3 examples/receive_zmq.py --port 9871
python3 examples/live_plot.py --port 9871      # live plot at http://127.0.0.1:8765
```

### Full measurements over UDP or JSONL

Every frame is also sent as one JSON datagram from `output.bind` to `output.target`. Start a receiver with `python3 examples/receive_udp.py --port 5601`. To write the same JSON to a file, one line per frame, set `output.jsonl_path`. The file is never overwritten.

Fields (defined in `src/tracker.rs`): `schema_version`, `instance`, `sequence`, `timestamp_ns` (Unix time at arrival), `elapsed_ns`, `width`, `height`, `capture_fps`, `processing_us`, `background_subtracted`, `left_minus_right_rad`, `left_plus_right_rad`, plus for each of `left`, `right`, `head` and `abdomen`:

- `enabled`, `angle_rad` (the reported edge), `image_angle_rad`
- `angles_rad` and `gradients`: every detected edge, strongest first
- `strength`, `peak_gradient` (strongest before the threshold), `coverage` (fraction of the sector inside the image), `intensity`, and `radius_px` (centroid only)

Missing or disabled angles are `null`, never zero or a stale value.

---

## Configuration reference

Every rig has one TOML file. The shipped files in [`configs/`](configs) are commented line by line; this is a summary.

| Key | Default | Meaning |
| --- | --- | --- |
| `instance` | `"rig-a"` | Label for the window, logs and recordings |
| `autosave` | `true` | Save GUI edits automatically |
| `preview_fps` | `40` | GUI refresh rate. Tracking always uses every frame |
| `source.kind` | `"camera"` | `"camera"`, `"video"`, `"network"` (FFmpeg) or `"demo"` (simulated fly) |
| `source.device` | `"auto"` | Camera selector ([choosing cameras](#choosing-cameras)) |
| `source.path`, `repeat`, `realtime` | | Video file, loop, and play at recorded speed (`false` = as fast as possible) |
| `source.width/height/fps` | `640/480/30` | Processing size and requested camera rate |
| `head`, `tail` | | Body axis anchor points (pixels) |
| `tracking.asymmetric` | `false` | Edit wings independently |
| `tracking.dark_on_light` | `true` | Dark insect on a light background |
| `tracking.subtract_background`, `background_path` | `false` | Track the difference from the saved background PNG |
| `tracking.angle_bins`, `radial_samples` | `240`, `72` | Sampling resolution of each sector |
| `tracking.<part>.method` | `"wing_blur"` (wings) | `"wing_blur"` robust, `"edge"` classic, `"centroid"` |
| `tracking.<part>.start_deg`, `span_deg` | `135`/`130` (left) | Sector angles: image degrees, clockwise from image-right |
| `tracking.<part>.inner_radius`, `outer_radius` | `55`, `185` | Sector radii (pixels) |
| `tracking.<part>.threshold` | `0.0` | Minimum edge strength. 0 reports the strongest edge |
| `tracking.<part>.max_edges`, `primary_edge` | `2`, `"strongest"` | Edges reported, and which one is the angle (`"toward_head"`, `"toward_tail"`) |
| `tracking.<part>.spike_width_deg`, `contrast_cap` | `5.0`, `0.12` | Robust detector: thinnest feature kept, and contrast limit |
| `output.zmq_enabled`, `zmq_bind` | `true`, `"tcp://*:9871"` | Kinefly-compatible publisher |
| `output.enabled`, `bind`, `target` | `true`, `127.0.0.1:5501` → `:5601` | Full UDP measurements |
| `output.jsonl_path` | `""` | Also write measurements to this file |
| `recording.format`, `quality` | `"mjpeg"`, `90` | Video format and MJPEG quality |
| `recording.directory`, `segment_mb`, `queue_frames` | `"recordings"`, `1024`, `32` | Where recordings go, file size, write buffer |
| `recording.enabled` | `false` | Record from the start |

Relative paths are relative to the config file. Edits that change the camera, outputs or label take effect after **Settings → Restart acquisition**. Everything else applies live.

---

## Command-line reference

```text
rekkefly                               open the GUI with the first free rig config
rekkefly VIDEO [--loop] [--fast]       track a video file
rekkefly --config FILE                 a specific rig config
rekkefly --rigs [rigs.toml]            start every rig in a manifest
rekkefly --list-cameras                show camera numbers, names and paths
rekkefly clip IN OUT [--start S] [--duration S] [--format mjpeg|png|raw] [--quality Q] [--every N]

Run-only overrides (never saved): --device SEL, --video FILE, --label NAME,
  --zmq-port P, --udp-port P, --udp-target IP:PORT, --record, --demo
Other: --headless, --frames N, --benchmark, --screenshot PNG, --init
```

Run `rekkefly --help` for the full text.

---

## Troubleshooting

| Problem | Fix |
| --- | --- |
| **Linux: `auto` failed, but `0` worked** (Rekke 0.5) | Fixed in 0.6. Rekkefly now skips the metadata-only `/dev/video*` nodes that USB cameras create, and tries cameras in order. Run `rekkefly --list-cameras` to see the numbers |
| **Linux: "Permission denied" for /dev/video0** | `sudo usermod -aG video $USER`, then log out and back in |
| **"Cannot open camera … may be in use"** | Close the other program or Rekkefly window that uses that camera, or give this rig another `device` |
| **"Outputs off: Bind output port …"** | Another program or rig uses that port, for example the old Kinefly bridge. Tracking keeps running in the window. Choose another port in Settings → Instance / output |
| **"… is already open in another Rekkefly window"** | Each config can be open in one place at a time. Close the other window, or use `--config` with another file |
| **The sample video plays instead of my camera** | No camera could be opened; the banner says why. Connect the camera and click **Retry camera** |
| **A video won't open** | AVI plays natively. Other formats need FFmpeg on your PATH, or set `REKKEFLY_FFMPEG=/path/to/ffmpeg` |
| **The edge jumps to a leg** | Make sure **Ignore legs** is ticked (`method = "wing_blur"`). Increase `spike_width_deg` for thick legs. Check that the inner radius excludes the body |
| **Recording stopped: "queue is full"** | The disk can't keep up. Use `format = "mjpeg"`, a lower resolution or frame rate, or a faster disk |

---

## Installing: details per system

### Mac: first launch

Rekkefly is free software without a paid Apple signature, so macOS may say it "cannot be opened". If you trust the download, run once in Terminal:

```sh
xattr -dr com.apple.quarantine "/Applications/Rekkefly.app"
```

This only clears the download flag on Rekkefly. It does not turn off Gatekeeper. Alternatively, try to open Rekkefly once, then use **System Settings → Privacy & Security → Open Anyway**. ([Apple's explanation](https://support.apple.com/en-au/102445).) Allow camera access when asked. A second window can be opened from the toolbar with **New window**.

Configs and backgrounds live in `~/Library/Application Support/Rekkefly/configs/`. Settings from an earlier Wingtrace install are imported on first launch.

### Windows

The installer puts Rekkefly under `%LOCALAPPDATA%\Programs\Rekkefly`, with Start-menu and desktop shortcuts, plus **Rekkefly - start all rigs**. Your configs live in `%APPDATA%\Rekkefly\configs` and are kept on uninstall. The build is unsigned, so SmartScreen may ask for confirmation. You can drag a video file onto the Rekkefly shortcut to open it.

### Ubuntu

`sudo apt install ./rekkefly_0.6.0_amd64.deb` puts `rekkefly` on your PATH and adds a menu entry with **Start all rigs** and **Open the sample video** actions. Configs live in `~/.config/rekkefly/configs`. Builds target Ubuntu 22.04 and newer on x86-64.

### Portable folders

The ZIP and tarball contain `rekkefly`, `configs/`, `samples/`, `examples/` and launchers: **Launch Demo**, **Launch Sample Video**, **Launch Camera A/B** and **Launch All Rigs** (`.command` on Mac, `.cmd` on Windows, `.sh` on Linux). Portable configs are read from the folder itself.

---

## Building, testing and benchmarks

Install [Rust](https://rustup.rs/) (stable). Windows also needs the MSVC build tools and NASM. Intel Macs need NASM (`brew install nasm`). On Ubuntu:

```sh
sudo apt-get install build-essential pkg-config nasm libclang-dev libv4l-dev libx11-dev libxi-dev \
  libxrandr-dev libxcursor-dev libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev libegl1-mesa-dev
```

```sh
git clone https://github.com/pvnkmrksk/rekke.git && cd rekkefly
cargo run --release -- samples/sample-fly.avi --loop    # try it
cargo test --locked                                     # 89 tests
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked --no-default-features    # processing only, no GUI/camera
```

**Benchmarks** measure processing only: foreground preparation and tracking, with capture, GUI and output excluded. The figures are for 640×480 with both wings at 240 × 72 samples, on a 4-core cloud VM:

```sh
./target/release/rekkefly --config configs/demo.toml --benchmark --frames 5000
```

| Detector | 0.5.1 | 0.6.0 |
| --- | --- | --- |
| Robust (`wing_blur`, default) | ≈ 600 µs/frame | **≈ 305 µs/frame** (≈ 3,300 frames/s) |
| Classic (`edge`) | ≈ 240 µs/frame | ≈ 235 µs/frame |

Sampling no longer copies or inverts the whole frame. Each sector is sampled once per frame from precomputed bilinear positions. These positions are rebuilt only when the geometry changes, not when you move a threshold.

**Regenerating assets:** `python3 scripts/brand.py` draws the icon, README banners, social card and DMG background (needs librsvg, ImageMagick and the Inter and Noto Sans Kannada fonts). Then `python3 scripts/icons.py` rasterizes `assets/rekkefly.svg` into every icon format. `REKKEFLY_TRACE=trace.csv cargo test --release --test intrusions trace -- --ignored` followed by `python3 scripts/trace_chart.py trace.csv docs/images/lr-trace` redraws the L − R chart. `rekkefly synth samples/sample-fly.avi --seconds 30 --fps 25 --quality 76 --noise 1.0` re-renders the synthetic sample. `REKKEFLY_FIGURE=docs/images/legs-classic-vs-robust.png cargo test --release --test intrusions figure -- --ignored` redraws the detector figure.

**Releases** are built and tested by GitHub Actions on Ubuntu, Windows and both Mac architectures. A `v*` tag publishes the installers after every platform passes. See [VALIDATION.md](VALIDATION.md) for what has and has not been verified.

### Project layout

| File | Role |
| --- | --- |
| `src/camera.rs` | Camera listing and selection, mode negotiation, fast grayscale paths |
| `src/source.rs` | Image sources: camera, native AVI, FFmpeg, simulated fly, sample discovery |
| `src/avi.rs` | AVI reader and writer (MJPEG / PNG / Y800) |
| `src/tracker.rs` | Polar sampling and the edge, robust and centroid detectors |
| `src/synth.rs` | Synthetic tethered fly with ground truth |
| `src/engine.rs` | Acquisition thread, background commands, fallback |
| `src/recording.rs`, `clip.rs` | Video recording; cutting and converting videos |
| `src/output.rs`, `zmq_output.rs`, `measurement_log.rs` | UDP, ZMQ and JSONL outputs |
| `src/rigs.rs`, `lock.rs`, `installation.rs` | Rig manifests, one-window-per-config locks, per-user configs |
| `src/config.rs`, `geometry.rs`, `persistence.rs` | Settings and validation, marker editing, autosave |
| `src/gui.rs` | The desktop interface |

Rekkefly was written from scratch after studying Kinefly's README and `tracker_edge.py`. It keeps Kinefly's annular-sector and angular-gradient approach and its angle conventions; it does not copy the Python code.
