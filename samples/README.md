# Sample video

`sample-fly.avi` plays whenever Rekke cannot open a camera, so a new user can
try every feature right after installing. It ships inside every package (the
Mac app, the Windows installer, the Ubuntu `.deb` and the portable folders).

The current file is a **synthetic** tethered fly rendered by Rekke itself:
30 s, 640×480, 25 fps, MJPEG, 6.6 MB. It has translucent wing stroke envelopes,
antennae and legs that kick through the wing sectors. Its wing angles are known
exactly, which the test suite uses to check the detector.

## Use your own footage instead

Cut 30 seconds from one of your recordings, for example a Wingtrace/Rekke AVI:

```sh
rm samples/sample-fly.avi
rekke clip "/Applications/Wingtrace/configs/recordings/rig-a-1791769898523379000-63855/video-0003.avi" \
    samples/sample-fly.avi --start 0 --duration 30 --quality 80
```

`rekke clip` reads AVI natively and needs no FFmpeg. It writes MJPEG, so a raw
640×480 recording shrinks about tenfold. If the file is still large (aim for
under ~20 MB in git), lower `--quality` (for example 70) or keep every second
frame with `--every 2`. Commit the new file and the next release ships it.

Without rebuilding, each user can also drop a clip at
`<Rekke config folder>/../samples/sample-fly.avi` or set `REKKE_SAMPLE=/path/to/clip.avi`.
For example, on a Mac that is `~/Library/Application Support/Rekke/samples/sample-fly.avi`.
That clip is then used instead of the bundled one.

To regenerate the synthetic clip:

```sh
rekke synth samples/sample-fly.avi --seconds 30 --fps 25 --quality 76 --noise 1.0
```
