# Validation — Rekkefly 0.6.0

This file records what was checked for this release, how, and what still needs checking. The GitHub workflow repeats the automated checks on Ubuntu, Windows, macOS Apple silicon and macOS Intel before any release is published.

## Checked in development (Linux x86-64, Ubuntu 24.04, Rust 1.97)

- `cargo fmt --check` passes. `cargo clippy --all-targets -- -D warnings` passes with default features and with `--no-default-features`.
- **89 tests pass** with default features (84 without GUI/camera); 9 report-only tests (comparison tables, figures) are ignored by default. Code that does not depend on the camera library was also type-checked for Windows (`x86_64-pc-windows-gnu`) and macOS (`aarch64-apple-darwin`). The camera feature needs each platform's C toolchain, so CI builds it there.
- The Linux `.deb`, tarball and ZIP were built with `scripts/package.py`, and the `.deb` was installed and smoke-tested:
  - `--list-cameras` works with no camera present.
  - The demo config and the bundled sample run headless (`/usr/share/rekkefly/samples/sample-fly.avi --fast`).
  - In a fresh home folder, a plain GUI launch seeds the user configs, finds no camera, falls back to the sample video and saves a screenshot.
  - Plain `rekkefly --rigs --demo --headless` starts both seeded rigs from the user folder.
- **Rig launcher:** three rigs from one manifest (`label`, `device`, ports) ran headless with `--demo --record`. Each wrote 60/60 frames to its own MJPEG recording (about 1.7 MB per 60 frames at 640×480, against about 18 MB raw). A recording was then replayed natively to JSONL with `--fast --headless`.
- **GUI:** screenshots of the main window, Settings, diagnostics and the no-camera banner were rendered under Xvfb and checked by eye (`docs/images`). Marker dragging is covered by the existing egui event tests.
- **Independent code review** of the new code checked the AVI reader against FFmpeg-made files: Y800, bgr24, pal8, bgra, MJPEG 4:2:2, PNG, audio-first files, a 1.15 GB OpenDML file and an interrupted FFmpeg recording. It also checked that FFmpeg reads Rekkefly's raw, PNG and MJPEG files at odd sizes. It found five bugs, all fixed with regression tests:
  - dropping a video could later save an older camera choice;
  - GRAY camera modes were ranked first but could not be requested;
  - launcher-started rigs could lose outputs silently;
  - `--device` on a video config dropped its image size;
  - seeking miscounted zero-length "repeat frame" chunks.

## Detector accuracy (synthetic ground truth)

`tests/intrusions.rs` renders backlit tethered flies with translucent stroke envelopes, antennae and six segmented legs. In some scenes the legs kick through the wing sectors or reach forward in front of the leading edge. Each scene has a known leading-edge angle.

| Scene (360 frames × 2 wings, 640×480) | Classic `edge`: frames > 4° off | Robust `wing_blur`: frames > 4° off |
| --- | --- | --- |
| Clean flight | 0 | 0 (median 0.24°) |
| Legs kicking | 108 / 720 | 0 |
| Front legs reaching | 720 / 720 | 0 |
| Faint wings + legs | 132 / 720 | 0 |
| Legs 2.5× thicker | 720 / 720 | 0 |
| Large amplitude and turning | 614 / 720 | 0 (worst 1.7°) |

**Narrow lines and L − R.** Six translucent lines per wing drift in and out of the blur. They are as dark as it, half as dark, twice as dark or 4 px wide. In other scenes a line hugs one wing's leading edge 3° outside, or runs 4° inside the blur. None of these produced a frame off by more than 4°, and L − R stayed within 1.8° of the truth. The classic detector biased L − R in up to 75% of frames (edge-hugging line) and spiked it by up to 101°. The robust detector ranks each edge by its steepness times the blur shoulder behind it (about 20°). Out-of-focus lines *wider* than the 5° filter remain a limit: with 12 px lines, 71 of 480 wing measurements were off by more than 4° (worst 34°). Pure gradient ranking gave 158 (worst 79°).

The robust results hold at 320×240 and 1280×960, with and without background subtraction, and for legs up to 7× thicker. Fast regression tests run on every build: the default detector has zero frames off by more than 4° across every leg kick, and the classic detector is shown to fail on the same frames. A separate test checks that per-sample inversion and subtraction give the same profiles and angles as building a full foreground frame.

**This is synthetic evidence, not anatomical validation.** Real recordings (different lighting, wing transparency, leg posture, focus and noise) are still needed. The user's own recording referenced during development was not available to this build, so `samples/sample-fly.avi` is synthetic. `samples/README.md` explains how to replace it with real footage.

## Performance

Processing only, 640×480, two wings, 240 × 72 samples, 4-core cloud VM. 0.5.1 and 0.6.0 were built and run alternately:

| Detector | 0.5.1 | 0.6.0 |
| --- | --- | --- |
| Robust `wing_blur` | 573–670 µs/frame | 295–325 µs/frame |
| Classic `edge` | 230–256 µs/frame | 225–283 µs/frame (same within noise) |

MJPEG encoding of a 640×480 grayscale frame at quality 90 takes about 2 ms on one core; the recorder spreads encoding across threads when frames queue up. Native MJPEG playback with tracking ran at about 450 frames/s.

## Not yet verified

- **Physical cameras.** No camera was attached during development. The Linux fix lists only `V4L2_CAP_VIDEO_CAPTURE` nodes, sorted by number, and tries them in order. It was checked against the nokhwa/v4l source and by unit tests of the selection logic, not on hardware. Please confirm `rekkefly --list-cameras` and `device = "auto"` on your rig.
- macOS and Windows camera capture, and the installers, are exercised only by CI smoke tests, which have no camera.
- Wingbeat-frequency estimation and stopped-flight detection are not implemented.

## Earlier releases

0.5.1 passed the native release workflow on all four platforms (run 38199683516), including installer and icon checks. Wingtrace 0.4.0 verified Kinefly-bridge ZMQ compatibility against `ros_zmq_bridge.py` with real PyZMQ subscribers, byte-exact raw AVI replay, and graceful multi-rig shutdown. Those protocol and recorder behaviours are unchanged in 0.6.0 except that MJPEG is now the default recording format.
