# Several rigs at once

One computer can run many rigs. Each rig needs its own **camera**, its own
**ports** and its own **config file**, which holds its markers, background and
recordings. Rekkefly checks all three before starting anything, so two rigs can
never fight over a camera or a port.

## Quick: one more window

Click **New window**, or start Rekkefly a second time without arguments. The new
window takes the next rig config:

| Window | Config | Camera | UDP send → receive | ZMQ |
| --- | --- | --- | --- | --- |
| First | `camera-a.toml` | 0 | 5501 → 5601 | 9871 |
| Second | `camera-b.toml` | 1 | 5502 → 5602 | 9872 |
| Third | `camera-c.toml` (created) | 2 | 5503 → 5603 | 9873 |

and so on. Each window locks its config, so the same rig cannot be opened twice.

## Repeatable: every rig with one command

Describe your rigs once in a **rig manifest**, `rigs.toml`:

```toml
[[rigs]]
label = "left-arena"       # window title, logs, recording folders
device = "0"               # first camera in `rekkefly --list-cameras`
zmq_port = 9871
udp_port = 5501
udp_target = "127.0.0.1:5601"

[[rigs]]
label = "right-arena"
device = "/dev/v4l/by-id/usb-046d_HD_Pro_Webcam_C920_ABC123-video-index0"
zmq_port = 9872
udp_port = 5502
udp_target = "127.0.0.1:5602"
record = true              # this rig records from the start
```

Then start them all:

```sh
rekkefly --rigs rigs.toml                     # one window per rig
rekkefly --rigs rigs.toml --headless --record # no windows, record everything
rekkefly --rigs rigs.toml --demo              # dry run with simulated flies
```

```text
Started left-arena   PID 4121    camera 0 · ZMQ tcp://*:9871 · UDP 127.0.0.1:5501 → 127.0.0.1:5601 [.../left-arena.toml]
Started right-arena  PID 4122    camera /dev/v4l/by-id/... · ZMQ tcp://*:9872 · UDP 127.0.0.1:5502 → 127.0.0.1:5602 [.../right-arena.toml]
Press Ctrl+C to stop every rig.
```

### Manifest keys

| Key | Meaning |
| --- | --- |
| `label` | Name in the window title, logs and recording folders |
| `config` | The rig's own settings file. Default `<label>.toml` beside `rigs.toml`. It is **created on first launch** if missing |
| `device` | Camera: `"auto"`, a number, a device path, or part of the camera's name ([choosing cameras](cameras.md#choosing-a-camera)) |
| `video` | Track a video file instead of a camera (path relative to `rigs.toml`) |
| `zmq_port` | Kinefly-format ZMQ publisher port |
| `udp_port` / `udp_target` | Local sending port, and the `IP:port` destination for full UDP measurements |
| `record` | Start recording when the rig starts |
| `enabled` | `false` skips the rig |

Values in `rigs.toml` apply to that launch. Everything else, such as markers,
detector settings and the background, lives in the rig's config and is saved
there as you edit.

### How the launcher behaves

- Each rig runs as its own process, so one rig's camera driver cannot stall another.
- Before starting anything, the launcher refuses manifests in which two rigs
  share a camera, a port, a label or a config file.
- If one rig fails to start or stops with an error, the others are stopped and
  the error is printed. An experiment never continues with a rig silently missing.
- **Ctrl+C** stops every rig and finishes every recording cleanly.

### Installed apps

Installed apps keep a ready-made `rigs.toml` next to your configs, so plain
`rekkefly --rigs` works:

| System | Your configs | All-rigs shortcut |
| --- | --- | --- |
| Mac | `~/Library/Application Support/Rekkefly/configs/` | `/Applications/Rekkefly.app/Contents/MacOS/rekkefly --rigs` |
| Windows | `%APPDATA%\Rekkefly\configs\` | Start menu → **Rekkefly - start all rigs** |
| Ubuntu | `~/.config/rekkefly/configs/` | App menu → Rekkefly → **Start all rigs** |

## Planning a lab

- **Cameras.** On Linux, use `/dev/v4l/by-id/...` paths so re-plugging cannot
  swap rigs. On Mac and Windows, distinguishing part of each camera's name is
  the most stable choice if the models differ.
- **Ports.** Give every rig its own ZMQ port and UDP sending port, and each
  experiment computer its own receiving port. The defaults count up from 9871,
  5501 and 5601.
- **USB bandwidth.** Several uncompressed cameras on one USB controller can
  exceed its bandwidth. Spread cameras over controllers, or use MJPEG camera
  modes.
- **CPU.** One 30 fps rig uses about 6% of one core
  ([performance](../tracking/performance.md)), so CPU is rarely the limit.

## Agents

The MCP tool `launch_rigs` starts every rig in a manifest, all or none, and
`stop_rig` stops them; see [AI agents](agents.md).
