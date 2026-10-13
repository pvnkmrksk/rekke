# Scripts and JSON

Everything Rekkefly does is available from the command line, with
machine-readable output, so experiments can be scripted end to end.

## JSON output

| Command | Prints |
| --- | --- |
| `rekkefly --list-cameras --json` | `{"cameras": [...]}`: each camera's `position` (the number to use as `device`), `name`, `id` (device path or system ID) and `backend_index` |
| `rekkefly track VIDEO --json` | The summary of a whole video (below) |
| `rekkefly --config rig.toml --headless --frames N --json` | Frames, rate, source, recording and output drop counts |
| `rekkefly --benchmark --json` | Tracking time per frame for a config |

`rekkefly track samples/bogong-moth.avi --frames 300 --json`:

```json
{
  "video": "samples/bogong-moth.avi",
  "frames": 300,
  "width": 640,
  "height": 480,
  "method": ["wing_blur", "wing_blur"],
  "video_markers": true,
  "timing": {"frames_per_second": 713.6, "seconds": 0.42, "mean_us": 449.1, "p99_us": 681.3, "max_us": 1600.3},
  "left": {"valid": 300, "mean": -20.68, "sd": 27.7, "min": -66.23, "max": 39.95},
  "right": {"valid": 300, "mean": -15.07, "sd": 20.76, "min": -57.56, "max": 18.8},
  "left_minus_right": {"valid": 300, "mean": -5.61, "sd": 11.64, "min": -27.49, "max": 38.93},
  "head": {"valid": 0, "mean": null, "sd": null, "min": null, "max": null},
  "abdomen": {"valid": 0, "mean": null, "sd": null, "min": null, "max": null}
}
```

`method` is the detector of each wing, and `video_markers` says whether the
video's own markers were used. With `--csv` or `--jsonl`, the summary also
names those files. Angles in summaries and CSV files are in **degrees**; `valid` counts the frames
where that angle was measured.

`rekkefly --benchmark --json`:

```json
{"frames": 300, "width": 640, "height": 480, "angle_bins": 240, "radial_samples": 72,
 "sectors": 2, "mean_us": 295.7, "frames_per_second": 3381.0}
```

## Exit codes

Rekkefly exits with status 0 on success and non-zero on any error, with the
reason on standard error. A missing camera, a busy port or an invalid config is
an error in headless runs, so a script never continues with a rig that is not
measuring.

## Examples

Track every video in a folder and collect the summaries (bash):

```sh
for v in trials/*.avi; do
  rekkefly track "$v" --config rig.toml --csv "${v%.avi}.csv" --json > "${v%.avi}.json"
done
```

The same in Python:

```python
import json, pathlib, subprocess

for video in sorted(pathlib.Path("trials").glob("*.avi")):
    out = subprocess.run(
        ["rekkefly", "track", str(video), "--config", "rig.toml",
         "--csv", str(video.with_suffix(".csv")), "--json"],
        check=True, capture_output=True, text=True).stdout
    summary = json.loads(out)
    print(video.name, summary["left_minus_right"]["mean"])
```

Run a rig for a fixed number of frames, recording, and check nothing was lost:

```sh
rekkefly --config rig.toml --headless --record --frames 18000 --json > run.json
python3 -c 'import json; r = json.load(open("run.json"))["recording"]; assert r["accepted"] == r["written"] and not r["error"], r'
```

## Configs are data

Rig configs are plain TOML, validated on load, so they can be generated or
edited by scripts. Writes from Rekkefly keep comments. To create a new config
with the default settings:

```sh
rekkefly --config new-rig.toml --init      # never overwrites an existing file
```

Load a config in Python (3.11 or newer) to read the markers:

```python
import tomllib
rig = tomllib.load(open("camera-a.toml", "rb"))
print(rig["tracking"]["left"]["hinge"])     # {'x': 296.0, 'y': 240.0}
```

For live, validated edits with comments kept, use the MCP tool `update_config`
([AI agents](agents.md)).
