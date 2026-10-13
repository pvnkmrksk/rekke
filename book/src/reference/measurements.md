# Measurement format

Every tracked frame produces one **measurement**. It is sent as one UDP datagram,
written as one line of a JSONL file (`output.jsonl_path`, `rekkefly track
--jsonl`), and summarised on ZMQ in the Kinefly format. The schema is defined by
`Measurement` in `src/tracker.rs`; `schema_version` changes if a field's meaning
ever does.

## Measurement

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | integer | Currently `1` |
| `instance` | string | The rig's label |
| `sequence` | integer | The source's frame number |
| `timestamp_ns` | integer | When the frame arrived, nanoseconds since 1970 (host time, not a camera timestamp) |
| `elapsed_ns` | integer | Time since the rig started, nanoseconds (0 in `rekkefly track`) |
| `width`, `height` | integer | Processing size, pixels |
| `capture_fps` | number | The source's measured frame rate (0 in `rekkefly track`) |
| `processing_us` | number | Time spent tracking this frame, microseconds |
| `background_subtracted` | boolean | Whether background subtraction was applied |
| `left`, `right`, `head`, `abdomen` | object | Each part's state (below) |
| `left_minus_right_rad` | number or null | L − R, radians |
| `left_plus_right_rad` | number or null | L + R, radians |

## Part state

| Field | Type | Meaning |
| --- | --- | --- |
| `enabled` | boolean | Whether the part is tracked |
| `angle_rad` | number or null | The reported angle: relative to the body axis, wings positive toward the head |
| `image_angle_rad` | number or null | The same edge as an image angle, clockwise from image-right |
| `angles_rad` | array of numbers | Every detected edge, the reported one first (up to `max_edges`) |
| `gradients` | array of numbers | Each edge's signed strength, in the same order |
| `strength` | number | The reported edge's strength |
| `peak_gradient` | number | The strongest edge before the threshold was applied: for setting `threshold` |
| `coverage` | number | Fraction of the sector inside the image, 0–1 |
| `intensity` | number | Mean normalized intensity in the sector, 0–1 |
| `radius_px` | number or null | Centroid method only: the mean radius |

Missing or disabled angles are `null`, never zero or a stale value.

## Example

One measurement from the Bogong moth, formatted:

```json
{
  "schema_version": 1,
  "instance": "rig-a",
  "sequence": 0,
  "timestamp_ns": 1791859983969667394,
  "elapsed_ns": 0,
  "width": 640,
  "height": 480,
  "capture_fps": 0.0,
  "processing_us": 1242.68,
  "background_subtracted": false,
  "left": {
    "enabled": true,
    "angle_rad": -0.37081718,
    "image_angle_rad": -0.38192773,
    "angles_rad": [-0.37081718, 0.28641963],
    "gradients": [-0.033694424, 0.0050776303],
    "radius_px": null,
    "intensity": 0.34237066,
    "strength": 0.033694424,
    "peak_gradient": 0.033694424,
    "coverage": 1.0
  },
  "right": {
    "enabled": true,
    "angle_rad": -0.2743702,
    "image_angle_rad": 3.4048524,
    "angles_rad": [-0.2743702, 0.4193797],
    "gradients": [0.0647008, -0.0024671927],
    "radius_px": null,
    "intensity": 0.37786517,
    "strength": 0.0647008,
    "peak_gradient": 0.0647008,
    "coverage": 1.0
  },
  "head": {"enabled": false, "angle_rad": null, "image_angle_rad": null, "angles_rad": [], "gradients": [], "radius_px": null, "intensity": 0.0, "strength": 0.0, "peak_gradient": 0.0, "coverage": 0.0},
  "abdomen": {"enabled": false, "angle_rad": null, "image_angle_rad": null, "angles_rad": [], "gradients": [], "radius_px": null, "intensity": 0.0, "strength": 0.0, "peak_gradient": 0.0, "coverage": 0.0},
  "left_minus_right_rad": -0.09644699,
  "left_plus_right_rad": -0.6451874
}
```

(`capture_fps` is 0 here because `rekkefly track` processes a file as fast as
possible; live sources report their rate.)

## Kinefly ZMQ message

| Field | Source |
| --- | --- |
| `x` | `left.angle_rad`, or `0.0` if missing |
| `y` | `right.angle_rad`, or `0.0` if missing |
| `yaw` | `x − y` |
| `z`, `pitch`, `roll` | `0.0` |
| `frame_number` | `sequence` |
| `timestamp_unix` | `timestamp_ns` in seconds |

The message is one ZMQ frame of JSON, written byte for byte as the Kinefly
Docker bridge writes it: the keys in the order `timestamp_unix`, `pitch`, `y`,
`x`, `yaw`, `z`, `roll`, `frame_number`, with `", "` and `": "` between them, and
numbers as Python writes them (`0.0`, `1e-05`):

```json
{"timestamp_unix": 1700000000.125, "pitch": 0.0, "y": -0.5, "x": 0.25, "yaw": 0.75, "z": 0.0, "roll": 0.0, "frame_number": 77}
```

## CSV from `rekkefly track`

| Column | Meaning |
| --- | --- |
| `frame` | Frame index in the video |
| `seconds` | The frame's time in the video |
| `left_deg`, `right_deg` | Wing angles, degrees |
| `left_minus_right_deg`, `left_plus_right_deg` | L − R and L + R, degrees |
| `head_deg`, `abdomen_deg` | Head and abdomen angles, degrees |
| `processing_us` | Tracking time, microseconds |

Missing values are empty cells.
