Rekke (ರೆಕ್ಕೆ, Kannada for wing) tracks the wings of tethered insects from cameras or video files. It is a native app and needs no ROS, Python or OpenCV.

## 0.6.0: double-click and track, many rigs, legs ignored

**Fixed**
- **Linux `device = "auto"` failed while `"0"` worked.** USB cameras also create metadata-only `/dev/video*` nodes, and the old code took the first node in the kernel's unsorted directory listing. Rekke now lists only nodes that capture video, in number order. `auto` tries each one until a camera opens, so busy cameras are skipped. `"1"` now means the second *camera*: in the common case that is `/dev/video2`. This change also makes the shipped `camera-b.toml` work on Linux.
- When a camera lacked the requested size, opening it failed instead of picking the nearest mode, because of a bug in the camera library's "closest" request. Rekke now picks the nearest supported mode itself, for automatic and explicit devices alike, and scales the image without distorting its aspect ratio.
- The Record button and `--record`/`--demo` no longer change the saved config, so the next launch doesn't start recording or use the demo source by surprise.
- With `--rigs … --demo`, the launcher passed conflicting arguments to each rig.

**New**
- **Ignore legs and antennae:** the robust wing detector is now the default (**Ignore legs** in the toolbar). It is also about 2× faster than before. On synthetic flies with reaching or kicking legs, the classic detector was off by more than 4° in 15–100% of frames; the robust one in none.
- **Sample video:** with no camera, a 30-second sample plays, with a clear banner and a **Retry camera** button. To use your own footage as the sample, run `rekke clip` (see `samples/README.md`).
- **Video files without FFmpeg:** AVI files (MJPEG, PNG, raw, uncompressed) play natively, including recordings cut short by a crash. Open one by dropping it on the window or with `rekke video.avi`. Use `--fast --headless` for offline analysis to JSONL.
- **`rekke clip`** cuts snippets and converts large raw recordings into small MJPEG or lossless PNG files.
- **Smaller recordings:** MJPEG by default (about 10× smaller than raw), encoded on parallel threads off the capture path. `format = "png"` or `"raw"` stays bit-exact.
- **Multi-rig manifests:** each `[[rigs]]` entry may set `label`, `device` (or `video`), `zmq_port`, `udp_port`, `udp_target` and `record`. Missing rig configs are created automatically, and shared cameras, ports or files are refused before anything starts.
- **One window per config:** a second window takes the next free rig config (and creates `camera-c.toml` and so on when needed), so double-clicking twice gives two independent rigs. A **New window** button does the same.
- Run-only overrides on the command line: `--device`, `--video`, `--label`, `--zmq-port`, `--udp-port`, `--udp-target`.
- **Resilience:** in the GUI, a busy output port turns that output off with a visible notice instead of stopping tracking. Headless runs and rigs still fail loudly.
- **Configs:** every key in the shipped configs is documented, and saving keeps your comments.
- **New icon:** a fly wing with its motion-blur stroke envelope.
- **Diagnostics:** the intensity-profile plot auto-scales and marks the detected edges. The window title shows the label, source and ZMQ port.

**Upgrading.** Existing configs keep working unchanged, including their detector choice. Tick **Ignore legs** to switch an existing rig to the robust detector. On Linux, check numeric `device` values with `rekke --list-cameras`. New configs use wider sectors (−45° to +85° around each wing) that contain a typical leading edge.

The detector results come from synthetic scenes. Real recordings and physical cameras on each operating system still need checking with your rigs; see VALIDATION.md.

Downloads: `Rekke-0.6.0-macos-arm64.dmg`, `Rekke-0.6.0-macos-x64.dmg`, `Rekke-0.6.0-windows-x64-setup.exe`, `rekke_0.6.0_amd64.deb`, plus portable ZIPs and a Linux tarball. Checksums are in `SHA256SUMS.txt`.
