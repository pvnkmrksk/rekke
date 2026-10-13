# The window

![The Rekkefly window tracking the Bogong moth, with the debug traces open](../images/main-window.png)

The window shows the camera image with the markers, the latest angles at the
lower left and a toolbar along the top. The title shows the rig's label, its
source and its ZMQ port, so you can tell several windows apart.

## Toolbar

| Control | What it does |
| --- | --- |
| **Save BG** | Saves the next frame as the background image. Capture an *empty* scene first |
| **H A L R** | Turns tracking of the head, abdomen, left and right wing on or off |
| **Asymmetric** | Edits each wing on its own instead of mirroring |
| **Subtract** | Tracks the difference from the saved background |
| **Invert** | For a bright insect on a dark background; the preview is inverted too |
| **Threshold L / R** | 0 reports the strongest edge, however faint. Raise it to reject noise. Applies live |
| **Ignore legs** | The robust wing detector, on by default ([why](../tracking/detectors.md)) |
| **Record** | Starts and stops video recording ([formats](recording.md)) |
| **New window** | Opens another Rekkefly window for the next rig, with its own camera and ports |
| **Settings** | Detector, precise geometry, camera, outputs, recording and sampling |

## Editing markers

Drag the dots; see [Placing the markers](../getting-started/markers.md). The
wings mirror each other until **Asymmetric** is on. Edits apply to the very
next frame.

## Settings

![The Settings window](../images/settings.png)

The Settings window has a part selector (**Left**, **Right**, **Head**,
**Abdomen**), the selected part's detector and geometry, and five sections:

| Section | Contents |
| --- | --- |
| *Selected part* | **Detector** (robust, classic, centroid), **Ignore features narrower than** and **Contrast cap** for the robust detector, **Primary edge**, **Threshold**, hinge X/Y, start and span, inner and outer radius, **Max edges**. Below them, **Show signal diagnostics** and the live peak and selected edge strengths |
| **Background** | Whether a background is loaded, **Load saved background**, and the background file's path |
| **Camera / input** | The source: a camera (with a list of cameras), a video file, a network stream or the simulated fly. **Back to saved source** and **Save as this config's source** for a video opened for one run |
| **Instance / output** | The instance label, the ZMQ publisher and its bind address, the UDP output's sending and receiving addresses, and the JSONL file |
| **Video / saved state** | **Automatically save configuration**, the recording folder, format, MJPEG quality, write queue and file size |
| **Sampling** | Angular bins, radial samples and the preview frame rate |

At the bottom: **Restart acquisition** applies source, label and output
changes, and **Reload config** discards unsaved edits by reading the file
again. Greyed fields are set for this run by the command line or the rig
launcher, and cannot be changed here.

## Debug traces

Tick **Settings → Show signal diagnostics**, or start with `--show plots`. Two
panels appear below the image:

- **Wing angles · last 8 s**: left (red), right (green) and **L − R** (white),
  from −90° to +90°. Spikes in L − R that the insect did not make are a sign of
  misplaced sectors or something in the sector that is not wing.
- **Angular profile** of the selected wing across its sector: thin is the raw
  intensity; bold is what the robust detector ranks edges on, with legs and
  antennae removed. Vertical lines mark the edges found; the labelled one is
  reported, with its angle.

## Saving

Edits are written 300 ms after you stop dragging, and when you close the window.
Writes are atomic (a new file replaces the old one in a single step), so a crash
cannot leave a half-written config, and the file's comments are kept. Turn
**Automatically save configuration** off to experiment without saving; edits then
last until the window closes.

Only one window may use a config file at a time. A second window started without
`--config` takes the next rig's config instead (`camera-b.toml`, then
`camera-c.toml`, which it creates).

## When something is wrong

Problems that stop tracking are shown in the window instead of closing it:

| Banner | Meaning |
| --- | --- |
| Amber: the example is playing | No camera could be opened; the banner says why. Click **Retry camera** |
| **Outputs off: Bind output port …** | Another program or rig uses that port. Tracking continues in the window; choose another port in **Instance / output** |
| **Recording stopped** | The disk could not keep up, or is full. Tracking continues |

Headless runs and rigs started from a manifest stop with an error instead, so an
experiment never runs silently without its outputs.
