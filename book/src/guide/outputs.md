# Receiving measurements

Every rig publishes every frame's measurement, as soon as it is tracked, in two
formats at once:

| Output | Transport | Content | Use it for |
| --- | --- | --- | --- |
| **Kinefly-compatible ZMQ** | ZMQ PUB on `tcp://*:9871` | Left, right and L − R in radians, frame number, time | Existing Kinefly subscribers; closed-loop control |
| **Full measurements** | UDP JSON to `127.0.0.1:5601`, and optionally a JSONL file | Everything: every part, every edge, strengths, timing | Analysis, logging, custom controllers |

Neither output ever slows tracking down. Both can be turned off, and their
addresses changed, in **Settings → Instance / output** or the config's
`[output]` table.

## Kinefly-compatible ZMQ

Each rig binds a ZMQ PUB socket (`output.zmq_bind`, default `tcp://*:9871`; the
second rig `9872`). Subscribe to everything (`b""`) and receive **one UTF-8 JSON
frame** per camera frame, byte for byte as the
[Kinefly Docker bridge](https://github.com/pvnkmrksk/Kinefly_Docker/blob/main/ros_zmq_bridge.py)
sends it:

```json
{"timestamp_unix": 1700000000.125, "pitch": 0.0, "y": -0.5, "x": 0.25, "yaw": 0.75, "z": 0.0, "roll": 0.0, "frame_number": 77}
```

| Field | Meaning |
| --- | --- |
| `x` | Left wing angle, radians |
| `y` | Right wing angle, radians |
| `yaw` | `x − y`: L − R, the steering signal |
| `z`, `pitch`, `roll` | Always `0.0`, as in the bridge |
| `frame_number` | The source's frame sequence number |
| `timestamp_unix` | When the frame arrived, seconds since 1970 |

A missing angle is sent as `0.0`, as the bridge did. Use the full measurements
when you need to tell a missing angle from a real zero.

Rekkefly replaces the bridge: don't run both on the same port. The publisher
speaks the ZeroMQ wire protocol the way libzmq does, ZMTP 3.0 or 2.0 as the
subscriber needs, so every ZeroMQ library works: pyzmq, libzmq, PlotJuggler,
and NetMQ in Unity, including older NetMQ releases that only speak ZMTP 2.0.
ZMQ PUB/SUB drops messages sent before a subscriber connects, and never
blocks tracking: like libzmq, a subscriber more than 1,000 messages behind
misses messages rather than slowing the rig down.

A receiver in Python (`python3 -m pip install pyzmq`):

```python
import json, zmq

socket = zmq.Context().socket(zmq.SUB)
socket.connect("tcp://127.0.0.1:9871")      # the rig's computer and port
socket.setsockopt(zmq.SUBSCRIBE, b"")
while True:
    m = json.loads(socket.recv_string())
    print(m["frame_number"], m["x"], m["y"], m["yaw"])
```

Ready-made versions ship in `examples/`:

```sh
python3 examples/receive_zmq.py --port 9871
python3 examples/live_plot.py --port 9871      # live plot at http://127.0.0.1:8765
```

Any language with a ZMQ library works the same way: connect a SUB socket,
subscribe to everything and parse each message as JSON.

To receive on another computer, connect to `tcp://<rig computer>:9871` and allow
that port through the rig computer's firewall. To keep the publisher private to
one computer, set `zmq_bind = "tcp://127.0.0.1:9871"`.

## Full measurements over UDP

Every frame is also sent as one JSON datagram from `output.bind` (this rig's own
sending port, `127.0.0.1:5501`) to `output.target` (your receiver,
`127.0.0.1:5601`). UDP needs no library and no connection, and a receiver can
start and stop at any time. A receiver using only Python's standard library:

```python
import json, socket

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", 5601))
while True:
    m = json.loads(sock.recvfrom(65535)[0])
    print(m["sequence"], m["left"]["angle_rad"], m["right"]["angle_rad"], m["left_minus_right_rad"])
```

or `python3 examples/receive_udp.py --port 5601`.

Each datagram is a full measurement:

```json
{"schema_version":1,"instance":"rig-a","sequence":0,"timestamp_ns":1791859983969667394,
 "elapsed_ns":0,"width":640,"height":480,"capture_fps":30.0,"processing_us":342.7,
 "background_subtracted":false,
 "left":{"enabled":true,"angle_rad":-0.3708,"image_angle_rad":-0.3819,
         "angles_rad":[-0.3708,0.2864],"gradients":[-0.0337,0.0051],"radius_px":null,
         "intensity":0.342,"strength":0.0337,"peak_gradient":0.0337,"coverage":1.0},
 "right":{…}, "head":{…}, "abdomen":{…},
 "left_minus_right_rad":-0.0964,"left_plus_right_rad":-0.6452}
```

Missing or disabled angles are `null`, never zero or a stale value. Every field
is described in [Measurement format](../reference/measurements.md).

## JSONL files

Set `output.jsonl_path` to also write every measurement to a file, one JSON
object per line, in the UDP format. The path is relative to the config; an
existing file is never overwritten. `rekkefly track --jsonl` writes the same
format offline.

## Timing

`timestamp_ns` and `timestamp_unix` are when the frame **arrived** at the
computer (host time), not a hardware exposure timestamp. `processing_us` is how
long tracking took for that frame; measurements are published as soon as it
finishes.
