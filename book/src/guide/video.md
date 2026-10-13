# Video files and offline analysis

Rekkefly tracks video files the same way it tracks cameras: watch them in the
window, run them through the live pipeline with its outputs, or track every
frame as fast as possible.

## Formats

| Format | Plays |
| --- | --- |
| AVI: MJPEG, PNG, raw 8-bit gray (Y800/GREY), uncompressed 8/24/32-bit, OpenDML (> 1 GB) | Natively, no extra software |
| Rekkefly and Wingtrace recordings, including ones cut short by a crash or power loss | Natively |
| MP4, MOV, M4V, MKV and anything else FFmpeg reads | With [FFmpeg](https://ffmpeg.org/download.html) on your `PATH`, or `REKKEFLY_FFMPEG=/path/to/ffmpeg` |

Videos of a different size are scaled to the configured processing size
**without changing their aspect ratio**, because stretching would bias every
angle.

## Watch a video

Drag a video onto the window, or:

```sh
rekkefly recording.avi            # play at the recorded speed
rekkefly recording.avi --loop     # loop it
rekkefly recording.avi --fast     # as fast as possible, outputs included
```

A video opened like this applies **to this run only**; the rig's saved camera
setup is untouched. **Settings → Camera / input** shows that the source is
temporary, with **Back to saved source** and **Save as this config's source**.

If a config with the same name sits beside the video (`recording.toml` for
`recording.avi`), its markers are used for that run, and are not saved over
yours. Keep one beside each experiment's video and anyone can replay it with
the right markers.

## Track a whole video: `rekkefly track`

```sh
rekkefly track recording.avi --config my-rig.toml --csv angles.csv
```

`track` decodes and tracks every frame as fast as possible (about 1,000 frames/s
at 640×480 on one core, decoding included), then prints a summary. It never
writes the config.

| Option | Meaning |
| --- | --- |
| `--config FILE` | Markers and detector settings. Without it, the video's own `.toml`, then the Bogong moth example's markers for the moth video, else the defaults |
| `--method robust\|classic` | Override the wing detector for both wings, for comparing detectors |
| `--csv FILE` | One row per frame: angles in degrees and tracking time |
| `--jsonl FILE` | Every full measurement, one JSON object per line, in the [UDP format](../reference/measurements.md) |
| `--frames N` | Stop after N frames |
| `--json` | Print the summary as JSON |

The CSV has a header and one row per frame:

```text
frame,seconds,left_deg,right_deg,left_minus_right_deg,left_plus_right_deg,head_deg,abdomen_deg,processing_us
0,0.0000,-21.246,-15.720,-5.526,-36.967,,,1242.7
1,0.0333,23.210,8.861,14.348,32.071,,,605.0
```

Empty cells are frames where that angle could not be measured, or the part is
off. `seconds` is the frame's time in the video.

To compare detectors on your own footage:

```sh
rekkefly track trial.avi --config rig.toml --method robust  --csv robust.csv
rekkefly track trial.avi --config rig.toml --method classic --csv classic.csv
```

## The live pipeline, offline

To replay a recording through exactly what a rig does, outputs included:

```sh
rekkefly --config my-rig.toml recording.avi --fast --headless
```

This publishes ZMQ and UDP and writes `output.jsonl_path` if set, as fast as
possible. Use it to test a closed-loop receiver against a known recording.

## Cut and shrink videos: `rekkefly clip`

`rekkefly clip` cuts a time range into a new AVI, and converts formats. It
never overwrites an existing file.

```sh
# 30 seconds starting 1 minute in, as small MJPEG
rekkefly clip video-0003.avi snippet.avi --start 60 --duration 30
# A whole raw recording as lossless PNG frames (about half the size)
rekkefly clip video-0000.avi video-0000-png.avi --format png
# Keep every 2nd frame
rekkefly clip big.avi half-rate.avi --every 2
# From a phone or screen recording (needs FFmpeg); every frame is kept
rekkefly clip phone.m4v moth.avi --duration 30 --quality 75
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--start S` | 0 | Seconds to skip |
| `--duration S` | to the end | Seconds to keep |
| `--format` | `mjpeg` | `mjpeg` (small), `png` (lossless) or `raw` (lossless, large) |
| `--quality Q` | 90 | MJPEG quality, 1–100 |
| `--every N` | 1 | Keep every Nth frame |

Output is 8-bit grayscale at the input's size. For inputs other than AVI,
FFmpeg only decodes, and Rekkefly cuts the range after decoding. (An FFmpeg
input-side seek silently dropped about half of a phone video's frames in
testing; decoding first keeps every one.)

## Agents and scripts

The MCP tools `track_video`, `snapshot` (with `video` and `frame`) and
`clip_video` do the same for agents; see [AI agents](agents.md).
