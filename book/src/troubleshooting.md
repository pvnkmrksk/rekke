# Troubleshooting

Find your symptom below. If it is not here, search the
[issues](https://github.com/pvnkmrksk/rekkefly/issues) or
[open one](https://github.com/pvnkmrksk/rekkefly/issues/new/choose): include
`rekkefly --version`, your system, and if you can, a short clip
(`rekkefly clip recording.avi clip.avi --duration 5`).

## Installing and starting

| Problem | Fix |
| --- | --- |
| **Mac: "Rekkefly cannot be opened"** | The app is not notarized. Run `xattr -dr com.apple.quarantine "/Applications/Rekkefly.app"` once, or use **System Settings → Privacy & Security → Open Anyway** ([details](getting-started/install.md#mac)) |
| **Windows: SmartScreen warning** | The installer is not code-signed. Choose **More info → Run anyway** |
| **Windows portable folder: a launcher closes at once** (0.6.0) | The 0.6.0 portable launchers call `rekke.exe`. Run `rekkefly.exe` directly, or edit the `.cmd` file to say `rekkefly.exe`. Fixed in the next release; the installer is not affected |
| **Ubuntu: the `.deb` will not install** | Use `sudo apt install ./rekkefly_<version>_amd64.deb` (with `./`), so apt fetches the dependencies. Ubuntu 22.04 or newer is needed |
| **"… is already open in another Rekkefly window"** | Each config can be open in one place at a time. Close the other window, or use `--config` with another file |

## Cameras

| Problem | Fix |
| --- | --- |
| **The moth plays instead of my camera** | No camera could be opened; the banner says why. Connect the camera and click **Retry camera** |
| **Mac: black image or no camera** | **System Settings → Privacy & Security → Camera**: turn Rekkefly (or your terminal) on |
| **Linux: "Permission denied" for /dev/video0** | `sudo usermod -aG video $USER`, then log out and back in |
| **Linux: `auto` failed, but `0` worked** (Rekke 0.5) | Fixed in 0.6: metadata-only `/dev/video*` nodes are skipped and cameras are tried in order. Check the numbers with `rekkefly --list-cameras` |
| **"Cannot open camera … may be in use"** | Close the other program or window using that camera, or give this rig another `device` |
| **The wrong camera opens after re-plugging** | Use a stable selector: a `/dev/v4l/by-id/...` path on Linux, or part of the camera's name ([choosing a camera](guide/cameras.md#choosing-a-camera)) |
| **The frame rate is lower than requested** | The camera may not offer that mode at that size, or the exposure is longer than the frame period. Shorten the exposure, or use a smaller size |
| **Two cameras on one USB hub drop out** | USB bandwidth. Use separate USB controllers or compressed (MJPEG) camera modes |

## Tracking

| Problem | Fix |
| --- | --- |
| **L − R is noisy or spiky** | Put the hinges on the wing bases, the inner radius just past the body and the outer radius inside the blur. Keep the sectors off the body, legs and head. Watch the debug traces while you adjust ([tuning](tracking/tuning.md)) |
| **The edge jumps to a leg** | Make sure **Ignore legs** is ticked (`method = "wing_blur"`). Increase `spike_width_deg` for thick legs |
| **The angle sits at the sector's limit** | The stroke goes beyond the sector: widen the span |
| **The angle is reported while the insect is not flying** | Raise `threshold` above the strength of the background noise (see the **peak** value in Settings) |
| **Angles look inverted or wrong on a bright insect** | Untick **Invert** (`dark_on_light = false`) for a bright insect on a dark background |
| **Angles changed after moving the camera** | Angles are relative to the body axis, so only the markers need moving: drag them back onto the insect |

## Video files

| Problem | Fix |
| --- | --- |
| **A video won't open** | AVI plays natively. Other formats need FFmpeg on your `PATH`, or `REKKEFLY_FFMPEG=/path/to/ffmpeg` |
| **A video plays with the wrong markers** | A `<video>.toml` beside the video provides its markers. Use `--config` to choose others |
| **`rekkefly clip` says the file exists** | It never overwrites. Choose another output name |

## Outputs

| Problem | Fix |
| --- | --- |
| **"Outputs off: Bind output port …"** | Another program or rig uses that port, for example the old Kinefly bridge. Tracking continues in the window. Choose another port in **Settings → Instance / output** |
| **My ZMQ subscriber receives nothing** | Subscribe to everything (`b""`), connect to the rig computer's address and port, and allow the port through its firewall. Messages sent before the subscriber connected are not delivered |
| **The run stopped: a ZMQ subscriber stalled** | A subscriber stopped reading for 500 ms. Make the receiver read continuously, in its own thread if needed |
| **UDP receiver gets nothing** | Check that `output.target` is the receiver's address and port, and that the receiver binds that port |

## Recording

| Problem | Fix |
| --- | --- |
| **Recording stopped: "queue is full"** | The disk cannot keep up. Use `format = "mjpeg"`, a lower resolution or frame rate, or a faster disk |
| **Recordings are too large** | Use MJPEG (the default) and lower `quality`; shrink old raw recordings with `rekkefly clip` |

## Still stuck?

Start Rekkefly from a terminal to see every message, and include them in an
[issue](https://github.com/pvnkmrksk/rekkefly/issues/new/choose).
