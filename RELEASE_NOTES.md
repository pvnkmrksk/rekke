**Rekkefly is the new Kinefly.** The name is ರೆಕ್ಕೆ (*rekke*, Kannada for wing) + fly, rooted in [Kinefly](https://github.com/ssafarik/Kinefly) and [Kinefly Docker](https://github.com/pvnkmrksk/Kinefly_Docker). It tracks the wings of tethered insects from cameras or video files, publishes Kinefly-compatible ZMQ, and needs no ROS, Python, OpenCV or Docker. Documentation: https://pvnkmrksk.github.io/rekkefly/docs/

## 0.6.0: Rekkefly. Double-click and track, many rigs, no L − R spikes from legs, ready for agents

**Renamed: Rekke is now Rekkefly.** The app, command (`rekkefly`), installers and settings folders use the new name. On first launch your Rekke 0.5 settings, markers and backgrounds are copied over automatically; recordings stay where they were. `REKKE_FFMPEG` and `REKKE_SAMPLE` still work. On Windows, Rekkefly installs beside Rekke 0.5; remove the old app in Settings → Apps. On Ubuntu, the `rekkefly` package replaces `rekke`.

**Fixed**
- **Linux `device = "auto"` failed while `"0"` worked.** USB cameras also create metadata-only `/dev/video*` nodes, and the old code took the first node in the kernel's unsorted directory listing. Rekkefly now lists only nodes that capture video, in number order. `auto` tries each one until a camera opens, so busy cameras are skipped. `"1"` now means the second *camera*: in the common case that is `/dev/video2`. This change also makes the shipped `camera-b.toml` work on Linux.
- When a camera lacked the requested size, opening it failed instead of picking the nearest mode, because of a bug in the camera library's "closest" request. Rekkefly now picks the nearest supported mode itself, for automatic and explicit devices alike, and scales the image without distorting its aspect ratio.
- The Record button and `--record`/`--demo` no longer change the saved config, so the next launch doesn't start recording or use the demo source by surprise.
- With `--rigs … --demo`, the launcher passed conflicting arguments to each rig.

**New**
- **No L − R spikes from legs, antennae or narrow lines.** The default detector now follows the wing's broad motion-blur *shoulder*: narrow features are removed, the median is taken across radial bands, and edges are ranked by the shoulder behind them. Legs kicking or reaching into the sectors, antennae, and lines as dark as the wing blur, inside or outside it or hugging one wing's edge, all left L − R within 1.5° of the truth on synthetic flies. The classic detector spiked by up to 116°. Turn it on or off with **Ignore legs** in the toolbar. It is also about 2× faster than its 0.5 version.
- **The Bogong moth example:** 30 seconds of a real tethered Bogong moth ships with every download. With no camera it plays with its own markers (scaled to your camera's size, never saved into your rig), behind a clear banner with a **Retry camera** button. Its hinges were measured from the video. On it, thin lines painted through the wings moved Rekkefly's L − R on 3 of 900 frames, and the classic detector's on 489. To use your own footage instead, see `samples/README.md`.
- **Agents and automation:** `rekkefly mcp` is a Model Context Protocol server (stdio, protocol 2024-11-05 to 2025-11-25). Through it, an AI agent can list cameras and configs, make validated config edits that keep comments, and start, stop and record rigs. It can also read live angles, measure every frame of a trial, see a snapshot with the tracking drawn on it, and track or cut videos. See `docs/mcp.md`. `--json` gives machine-readable output for `--list-cameras`, `--benchmark` and `--headless`.
- **`rekkefly track`** tracks a whole video as fast as possible, about 1,000 frames/s at 640×480 including decoding. It prints a summary of every angle and can write a CSV, the full measurements as JSONL, or JSON. A video opened for one run uses a `<video>.toml` beside it for its markers.
- **Leading edge by polarity:** `primary_edge = "toward_head"` now reports where the wing gives way to background on the head side. A stray feature beyond the wing can no longer take its place.
- **Debug traces:** the diagnostics show L − R beside the left and right traces, with a degree scale. They also show what the robust detector sees, the profile with legs and antennae removed, over the raw profile, and they label the reported edge.
- **Video files without FFmpeg:** AVI files (MJPEG, PNG, raw, uncompressed) play natively, including recordings cut short by a crash. Open one by dropping it on the window or with `rekkefly video.avi`. Use `--fast --headless` for offline analysis to JSONL.
- **`rekkefly clip`** cuts snippets and converts large raw recordings into small MJPEG or lossless PNG files. From MP4, MOV or M4V it now keeps every frame (an FFmpeg input-side seek used to drop about half of a phone video's frames).
- **Smaller recordings:** MJPEG by default (about 10× smaller than raw), encoded on parallel threads off the capture path. `format = "png"` or `"raw"` stays bit-exact.
- **Multi-rig manifests:** each `[[rigs]]` entry may set `label`, `device` (or `video`), `zmq_port`, `udp_port`, `udp_target` and `record`. Missing rig configs are created automatically, and shared cameras, ports or files are refused before anything starts.
- **One window per config:** a second window takes the next free rig config (and creates `camera-c.toml` and so on when needed), so double-clicking twice gives two independent rigs. A **New window** button does the same.
- Run-only overrides on the command line: `--device`, `--video`, `--label`, `--zmq-port`, `--udp-port`, `--udp-target`.
- **Resilience:** in the GUI, a busy output port turns off only that output, with a visible notice, instead of stopping tracking. Headless runs and launcher-started rigs still fail loudly.
- Cameras that offer grayscale modes can now use them (fewer conversions), and switching a video config to a camera keeps its image size.
- **Configs:** every key in the shipped configs is documented, and saving keeps your comments.
- **New look:** a tethered fly with both wings' stroke envelopes and their leading edges in coral, which is exactly what Rekkefly measures.
- The window title shows the label, source and ZMQ port.

**Upgrading.** Existing configs keep working unchanged, including their detector choice. Tick **Ignore legs** to switch an existing rig to the robust detector. On Linux, check numeric `device` values with `rekkefly --list-cameras`. New configs use wider sectors (−45° to +85° around each wing) that contain a typical leading edge.

The detector results come from synthetic flies with known angles and from the real Bogong moth footage. Physical cameras on each operating system still need checking with your rigs; see VALIDATION.md.

Downloads: `Rekkefly-0.6.0-macos-arm64.dmg`, `Rekkefly-0.6.0-macos-x64.dmg`, `Rekkefly-0.6.0-windows-x64-setup.exe`, `rekkefly_0.6.0_amd64.deb`, plus portable ZIPs and a Linux tarball. Checksums are in `SHA256SUMS.txt`.
