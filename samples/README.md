# The Bogong moth example

`bogong-moth.avi` is 30 seconds of a real tethered Bogong moth (*Agrotis
infusa*), filmed from above at 30 fps with its head down. It ships with every
package (the Mac app, the Windows installer, the Ubuntu `.deb` and the portable
folders) and plays whenever Rekkefly cannot open a camera, so a new user can try
every feature right after installing.

640×480, 900 frames, MJPEG quality 75, 10 MB. Converted from the original
H.264 clip with `rekkefly clip`.

Why it makes a good test:

- **Antennae and legs** sit right where the sectors end toward the head. Extend a
  sector over them and a classic detector starts reporting them as the wing.
- **One wing is faint.** The moth's left wing (image right) is often barely lit.
- **Strobing.** At 30 fps each frame catches a different part of the wingbeat,
  so left and right swing by tens of degrees between frames while L − R stays
  small, because both wings beat together.

Its markers and sectors live in `configs/bogong-moth.toml`. The same values are
built into Rekkefly, so the example always plays with markers that fit the moth,
whatever size your camera config is. Those markers are never saved into your
rig's config.

Try it:

```sh
rekkefly samples/bogong-moth.avi --loop            # its own markers, for this run
rekkefly --config configs/bogong-moth.toml         # the example config; edits are saved
rekkefly samples/bogong-moth.avi --headless --fast # how fast it tracks
```

## Use your own footage instead

Put a clip at `<your Rekkefly folder>/samples/sample.avi` (on a Mac:
`~/Library/Application Support/Rekkefly/samples/sample.avi`), or set
`REKKEFLY_SAMPLE=/path/to/clip.avi`. It then plays instead of the moth.
A config with the same name beside it (`sample.toml`) gives it its own markers.
Any video opened with `rekkefly <video>` picks up a `<video>.toml` the same way.

Cut 30 seconds from one of your recordings:

```sh
rekkefly clip recording.avi sample.avi --start 60 --duration 30 --quality 75
```

`rekkefly clip` reads AVI natively. MP4, MOV and other formats go through
FFmpeg, which only decodes; Rekkefly cuts the time range after decoding, so no
frames are lost from phone or screen recordings with irregular timestamps. The
output is MJPEG: a raw 640×480 recording shrinks about tenfold. To make it
smaller, lower `--quality` (the moth tracks the same at 75 as losslessly) or keep every second frame
with `--every 2`.

To render the synthetic fly used by the test suite (known wing angles, legs and
antennae):

```sh
rekkefly synth synthetic-fly.avi --seconds 30 --fps 25 --quality 76 --noise 1.0
```
