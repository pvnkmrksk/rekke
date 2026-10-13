# Evidence

Two kinds of tests support Rekkefly's tracking: **real footage**, where the
detector is compared with itself on the same frames with and without added
intrusions, and **synthetic flies** with exactly known wing angles. Every
number here can be reproduced with the commands given, and faster versions of
each test run on every build.

## On the real Bogong moth

Two thin bright lines, like antennae, were painted onto every frame of the
[Bogong moth example](../getting-started/bogong-moth.md). They start at the head
and sweep through both wing sectors over time. Each detector is compared with
its own result on the same frame without them:

![The same moth frame with painted antennae: the classic detector follows the antennae, Rekkefly stays on each wing's leading edge](../images/moth-antenna-frame.png)

<img class="only-light" src="../images/moth-antennae-light.png" alt="Over 30 seconds of the real moth with painted antennae, the classic detector's L minus R moves by more than 5 degrees in 489 of 900 frames, often by over 100 degrees; Rekkefly's moves in 3 of 900.">
<img class="only-dark" src="../images/moth-antennae-dark.png" alt="Over 30 seconds of the real moth with painted antennae, the classic detector's L minus R moves by more than 5 degrees in 489 of 900 frames, often by over 100 degrees; Rekkefly's moves in 3 of 900.">

| Real Bogong moth, 900 frames | Classic: L − R moved > 5° | Rekkefly: L − R moved > 5° |
| --- | --- | --- |
| Antennae 2 px wide, gray 150 | 489 | **3** |
| Antennae 3 px, gray 200 | 623 | **3** |
| Faint antennae 1.5 px, gray 110 | 315 | **2** |
| Legs 5 px wide, gray 170 | 632 | **8** |

**Image perturbations** (every third frame, 300 frames): Rekkefly's L − R moved
by more than 5° on at most 2 frames in every case.

| Perturbation | Classic | Rekkefly |
| --- | --- | --- |
| Darker by 25% | 2 | 2 |
| Brighter by 30% | 1 | 0 |
| Sensor noise, σ = 4 | 20 | 0 |
| JPEG recompression at quality 40 | 11 | 2 |
| Blur, σ = 1.5 px | 11 | 1 |

**Without perturbations**, Rekkefly's L − R spreads less (sd 11.1° against
14.4° over the whole clip), and left and right agree better (correlation 0.94
against 0.86 over 300 frames).

```sh
cargo test --release --test moth -- --ignored --nocapture
```

A painted antenna is a controlled stand-in: because it is drawn onto real
frames, each detector can be compared with itself on the same frame without it.
There is no ground truth for the moth's wing angles, so accuracy on real
footage is shown by robustness, stability and agreement between the wings.

## On synthetic flies with known angles

`tests/intrusions.rs` renders backlit tethered flies with translucent stroke
envelopes, antennae and six segmented legs, with known leading-edge angles. In
some scenes the legs kick through the wing sectors or reach forward in front of
the leading edge; in others, narrow lines drift in and out of the blur.

<img class="only-light" src="../images/lr-trace-light.png" alt="L minus R over 12 seconds: the classic detector spikes to plus or minus 100 degrees and is biased by a line beside one wing; Rekkefly follows the ground truth within 1.2 degrees.">
<img class="only-dark" src="../images/lr-trace-dark.png" alt="L minus R over 12 seconds: the classic detector spikes to plus or minus 100 degrees and is biased by a line beside one wing; Rekkefly follows the ground truth within 1.2 degrees.">

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

The results hold at 320×240 and 1280×960, with or without background
subtraction, and for legs up to 7× thicker. The median angle error stays near
0.25°.

```sh
cargo test --release --test intrusions -- --ignored --nocapture
```

**Where it still fails.** Out-of-focus lines *wider* than the 5° filter: with
12 px lines, 71 of 480 wing measurements were off by more than 4° (worst 34°).
Ranking by steepness alone gave 158 (worst 79°).

## What has not been verified

- Fly footage with known angles, and recordings from your own rigs.
- Physical cameras on each operating system: development had no camera
  attached, and CI has none. Please confirm `rekkefly --list-cameras` and
  `device = "auto"` on your rig.

The full record of what was checked for each release is the
[Validation report](../validation.md).
