# Performance

Rekkefly is fast enough that tracking is never the bottleneck of a rig: a
camera at 30 fps uses about 6% of one core.

<img class="only-light" src="../images/throughput-light.png" alt="Total frames per second with every rig decoding and tracking its own Bogong moth video: about 970 for one rig, 2,040 for two, 3,960 for four and 3,770 for eight on a 4-core machine, far above the 240 frames per second that eight 30 fps cameras need.">
<img class="only-dark" src="../images/throughput-dark.png" alt="Total frames per second with every rig decoding and tracking its own Bogong moth video: about 970 for one rig, 2,040 for two, 3,960 for four and 3,770 for eight on a 4-core machine, far above the 240 frames per second that eight 30 fps cameras need.">

Measured on a 4-core 2.8 GHz Xeon cloud machine, 640×480, both wings at
240 × 72 samples:

| | Rekkefly 0.6 |
| --- | --- |
| Tracking one frame, robust detector (moth) | **≈ 340 µs** (99% under 0.7 ms) |
| Tracking one frame, classic detector (moth) | ≈ 210 µs |
| Video, one rig, MJPEG decoding included | ≈ 800–1,000 frames/s |
| Several rigs at once, totals | ≈ 2,000 frames/s for two, 3,500–4,000 for four or eight |
| One rig at 30 fps, real time | 6% of one core, 10 MB of memory |
| Program size (Linux) | 14 MB, one file |

Against the previous version, on a synthetic 640×480 frame:

| Detector | 0.5.1 | 0.6.0 |
| --- | --- | --- |
| Robust (`wing_blur`) | ≈ 600 µs/frame | **≈ 310 µs/frame** (≈ 3,200 frames/s) |
| Classic (`edge`) | ≈ 240 µs/frame | ≈ 235 µs/frame |

## Why it is fast

- **Tracking runs on each rig's capture thread**, as soon as a frame arrives.
  There is no queue between the camera and the tracker to add latency.
- **No frame copies.** Each sector is sampled once per frame from precomputed
  bilinear positions. Inversion and background subtraction are applied while
  sampling, not to a copy of the frame. The positions are rebuilt only when the
  geometry changes, not when a threshold moves.
- **Recording is off the capture path.** The capture thread hands each frame
  to encoder threads, which run in parallel only when frames queue up.
- **The GUI only draws.** The preview refreshes at `preview_fps`; tracking uses
  every frame regardless.
- **Native code.** Rust, compiled for each platform, with SIMD JPEG decoding
  and encoding.

## What costs time

Tracking time grows with the number of samples per sector,
`angle_bins × radial_samples`, and with the number of enabled parts. The robust
detector takes about 30–60% longer than the classic one, for its bands,
filtering and ranking. Frame size matters only for decoding and recording: tracking samples
the same number of points whatever the resolution.

## Measure your own

```sh
rekkefly --config my-rig.toml --benchmark --frames 5000   # tracking only, synthetic frame
rekkefly track my-recording.avi --config my-rig.toml      # decoding + tracking
cargo test --release --test moth throughput -- --ignored --nocapture   # 1, 2, 4, 8 rigs
```

`--benchmark` measures processing only (foreground preparation and tracking),
without capture, the GUI or outputs, and does not open a camera. Each
measurement also carries `processing_us`, the tracking time of that frame, so
live performance can be logged too.
