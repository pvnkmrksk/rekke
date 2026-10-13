# Quick start

From download to wing angles in five minutes.

## 1. Open Rekkefly

Double-click Rekkefly. It opens the first camera it can. If no camera can be
opened, the [Bogong moth example](bogong-moth.md) plays with its own markers,
and an amber banner says why. You can learn everything on the moth first, then
connect a camera and click **Retry camera**.

![No camera: Rekkefly plays the Bogong moth example with its own markers and says so in an amber banner](../images/no-camera.png)

## 2. Light the insect and focus

Rekkefly measures the **motion-blur envelope** of each wing: the translucent fan
a beating wing leaves in a single exposure. Typical rigs film from above or
below with the insect backlit, so the body and the blur are darker than the
background. Then:

- **Exposure.** Long enough that each frame shows a smooth blur over many
  wingbeats, short enough that the background is not saturated.
- **Focus** on the wing hinges.
- **Contrast.** The blur should be clearly different from the background. Tick
  **Invert** if your insect is bright on a dark background.

## 3. Place the markers

Drag the dots onto the insect:

| Dot | Put it on |
| --- | --- |
| Magenta / cyan | The head and the tail. Together they define the body axis |
| Red / green (inner) | The left and right **wing hinges**: where each wing turns, at its base |
| Corner dots | The sector's angular limits and outer radius, in one drag |
| Middle dots on an arc | Only the inner or the outer radius |
| White centre dot | Moves the whole setup |

Each sector is an annulus around its hinge. Set the **inner radius just past
the body** and keep the **outer radius inside the wing blur**. The angular
limits should cover the whole stroke with a little margin, while staying clear
of the body, the legs and the head. The wings mirror each other while you edit;
tick **Asymmetric** to edit them separately.

[Placing the markers](markers.md) explains each choice, with pictures.

## 4. Save a background (optional)

For a cluttered scene, take the insect out, click **Save BG**, put the insect
back and tick **Subtract**. Rekkefly then tracks the difference from the empty
scene. With a clean backlit background this is rarely needed.

## 5. Read the angles

The box at the lower left shows the left and right wing angles and L − R, in
radians. Angles are 0 when a wing points straight out from the body axis and
positive toward the head, as in Kinefly.

The same values are published for every frame:

- on **ZMQ port 9871**, in the Kinefly Docker bridge's JSON format, so
  existing Kinefly subscribers keep working;
- over **UDP** to `127.0.0.1:5601`, with every detail of the measurement.

Try a receiver:

```sh
python3 -m pip install pyzmq
python3 examples/receive_zmq.py --port 9871
python3 examples/live_plot.py --port 9871     # live plot at http://127.0.0.1:8765
```

See [Receiving measurements](../guide/outputs.md) for both formats and for
receivers in other languages.

## 6. Check the tracking

Open **Settings → Show signal diagnostics**. Watch L − R while the insect
flies: it should be smooth, without spikes when the legs move. The angular
profile panel shows exactly which edge was chosen. If something looks wrong,
[Tuning a rig](../tracking/tuning.md) walks through the fixes.

## Everything is saved

Every change is written to the rig's config file 300 ms after you stop
dragging, and when you close the window. Close Rekkefly and open it again:
everything is where you left it, including the background. Config files are
plain, commented TOML; see [Configuration](../reference/configuration.md).

## Next

- A second camera: click **New window** ([several rigs](../guide/rigs.md)).
- Record the session: click **Record** ([recording](../guide/recording.md)).
- Analyse a recording offline: `rekkefly track recording.avi --csv angles.csv`
  ([video files](../guide/video.md)).
