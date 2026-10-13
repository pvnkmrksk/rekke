# The Bogong moth example

Every download includes 30 seconds of a real tethered **Bogong moth**
(*Agrotis infusa*), filmed from above at 30 fps with its head down:
`samples/bogong-moth.avi` (640×480, 900 frames, MJPEG, 10 MB). It plays
whenever Rekkefly cannot open a camera, so you can learn every feature right
after installing, and it is the real-footage test behind Rekkefly's
[evidence](../tracking/evidence.md).

![Rekkefly tracking the Bogong moth with the debug traces open](../images/main-window.png)

## Try it

```sh
rekkefly samples/bogong-moth.avi --loop        # watch it; the video brings its own markers
rekkefly --config configs/bogong-moth.toml     # the example config; your edits are saved
rekkefly track samples/bogong-moth.avi         # every frame, as fast as possible
```

In an installed app the files are in the app's folders (for example
`/usr/share/rekkefly/samples/` on Ubuntu); portable folders have them beside
`rekkefly`. On Ubuntu the app menu also has **Open the Bogong moth example**.

`rekkefly track` prints a summary:

```text
Tracked 900 frames of bogong-moth.avi (640×480, robust detector, the video's own markers) in 0.87 s: 1032 frames/s, 340 µs tracking per frame (99% under 667 µs)
  left     mean   -20.9°  sd  28.0°  range  -66.2° …  +40.0°  (900 frames)
  right    mean   -15.1°  sd  21.0°  range  -59.6° …  +19.8°  (900 frames)
  L−R      mean    -5.9°  sd  11.1°  range  -27.5° …  +38.9°  (900 frames)
```

## Why it is a good test

- **Antennae and legs** reach out beside the wing sectors near the head. Extend
  a sector over them and a classic detector starts reporting them as the wing.
- **One wing is faint.** The moth's left wing (image right) is often barely lit.
- **Strobing.** The wingbeat is faster than the camera, so each frame catches a
  different moment of it. L and R swing by tens of degrees between frames, while
  L − R, which compares the two wings at the same moment, stays steady.
- **Bright on dark.** The moth is lit from above, so it is brighter than the
  background: `dark_on_light = false`. The display inverts it, so every rig
  looks the same on screen.

## How its markers were placed

The markers were measured from the video, not placed by eye:

| Marker | Value | How |
| --- | --- | --- |
| Hinges | (371.5, 239) and (334.5, 239) | Straight lines fitted to one wing's leading edge in 190 frames all pass within 0.7 px (median) of one point, (334.5, 239). The other wing's fits gave (370, 242); the hinges were set symmetrically about the body axis |
| Inner radius | 20 px | Just past the body |
| Outer radius | 150 px | Inside the wing blur |
| Sector | −70° to +55° (image degrees, each wing mirrored) | Clear of the thorax toward the tail, past the most forward stroke, short of the legs |
| Leading edge | `primary_edge = "toward_head"` | Where the wing gives way to background on the head side |

Compared with hinges placed by eye 20 px away, this halved the L − R noise.
Starting the sectors at −75° instead of −70° let one sector clip the thorax and
caused L − R spikes. The full config, with comments, is
[`configs/bogong-moth.toml`](https://github.com/pvnkmrksk/rekkefly/blob/main/configs/bogong-moth.toml).

![Settings on the Bogong moth: the robust detector, leading edge toward the head, hinges at the wing bases](../images/settings.png)

## The example's markers are never saved

When the example plays because no camera opened, it uses its own markers,
scaled to your camera's processing size, and shows a banner. Your rig's own
markers are untouched: nothing from the example is ever saved into your
config. Click **Retry camera** once a camera is connected.

The same rule applies to any video: opening `recording.avi` for one run picks
up `recording.toml` beside it, if there is one, for that run only.

## Use your own footage as the example

Put a clip at `<your Rekkefly folder>/samples/sample.avi` (on a Mac,
`~/Library/Application Support/Rekkefly/samples/sample.avi`), or set the
environment variable `REKKEFLY_SAMPLE=/path/to/clip.avi`. It then plays instead
of the moth. A `sample.toml` beside it gives it its own markers.

Cut 30 seconds from a recording:

```sh
rekkefly clip recording.avi sample.avi --start 60 --duration 30 --quality 75
```

At quality 75 the moth tracks the same as from lossless frames, at a tenth of
the size.
