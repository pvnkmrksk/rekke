# Coming from Kinefly

Rekkefly does the same job as Kinefly, keeps its conventions and speaks the
Kinefly Docker bridge's ZMQ format. Moving an existing setup takes minutes.

## Two projects Rekkefly stands on

**[Kinefly](https://github.com/ssafarik/Kinefly)** made real-time wing tracking
practical for tethered-flight rigs. It measures the leading edge of each wing's
motion-blur stroke envelope inside annular sectors around the wing hinges,
together with head and abdomen angles, and publishes the results for
closed-loop experiments. Many labs built their arenas around it. Kinefly was
written for ROS 1 and Python 2, both now end-of-life, and it is no longer
maintained. Installing it today means fighting old operating systems and
dependency chains (the
[2015 installation notes](https://peterthomasweir.blogspot.com/2015/07/installation-notes-for-kinefly.html)
give a flavour), and the old stack can be fragile in long experiments.

**[Kinefly Docker](https://github.com/pvnkmrksk/Kinefly_Docker)** kept Kinefly
alive. It packed the original ROS stack into a container and added
`ros_zmq_bridge.py`, which re-publishes the wing angles as simple JSON over ZMQ,
so any language could listen without ROS. Experiments ran on it; its message
format is the contract Rekkefly keeps.

**Rekkefly** is the next step: a from-scratch native app that keeps what worked
and drops the stack.

| | Kinefly | Kinefly Docker | Rekkefly |
| --- | --- | --- | --- |
| Install | ROS 1 + Python 2 + OpenCV on an old Ubuntu | Docker, X forwarding, camera pass-through | One installer or app (Mac, Windows, Linux) |
| Language | Python 2 | Python 2 in a container | Rust, compiled to one native program |
| Cameras | ROS camera drivers | ROS camera drivers in the container | Native USB/webcam, video files, network streams |
| Output | ROS topics | ZMQ JSON via the bridge | **The same ZMQ JSON**, plus full UDP/JSONL measurements |
| Legs and antennae in the wing region | Can pull the edge | Can pull the edge | Ignored ([evidence](tracking/evidence.md)) |
| Several rigs | One ROS launch per rig | One container per rig | One TOML, one command, one window per rig |
| Recording | ROS bags | ROS bags | MJPEG/PNG/raw AVI with exact frame times |
| Automation | ROS services | — | MCP server for agents, `--json` CLI |
| Maintained | No | Bridge only | Yes |

## What stays the same

- **Geometry.** Annular sectors around each wing hinge, a head–tail body axis,
  and head and abdomen sectors.
- **Angles.** Measured from the outward perpendicular to the body axis, positive
  toward the head, in radians on the wire.
- **The measure.** The leading edge of the stroke envelope, from the angular
  intensity gradient (`primary_edge = "toward_head"`).
- **The message.** Byte for byte what the bridge sends: the same fields and
  values, and Python 2's `json.dumps` key order, spacing and number format:

  ```json
  {"timestamp_unix": 1700000000.125, "pitch": 0.0, "y": -0.5, "x": 0.25, "yaw": 0.75, "z": 0.0, "roll": 0.0, "frame_number": 77}
  ```

  `x` and `y` are the left and right wing angles, `yaw = x − y`, on
  `tcp://*:9871` by default.

## Moving a rig

1. **Install Rekkefly** on the rig computer ([Install](getting-started/install.md)).
2. **Stop the bridge container.** Rekkefly publishes on the same port; both
   cannot bind it.
3. **Open Rekkefly** and choose the rig's camera
   ([Cameras](guide/cameras.md)).
4. **Place the markers** by dragging them onto the fly. Kinefly's settings are
   not imported, but the geometry is the same, so the same placement applies:
   hinges on the wing bases, inner radius just past the body, outer radius inside
   the blur ([Placing the markers](getting-started/markers.md)).
5. **Choose the leading edge** in Settings: **Primary edge → Toward head**
   (`primary_edge = "toward_head"`) to match Kinefly's measure.
6. **Start your experiment code unchanged.** It connects to port 9871 and
   receives the same messages.

To compare the two before switching, record a session in Rekkefly and track it
with both detectors:

```sh
rekkefly track session.avi --config rig.toml --method classic --csv classic.csv   # Kinefly's strongest edge
rekkefly track session.avi --config rig.toml --method robust  --csv robust.csv    # Rekkefly's default
```

## What is different

| | Kinefly | Rekkefly |
| --- | --- | --- |
| Default wing detector | Strongest gradient | Robust: ignores legs and antennae ([how](tracking/detectors.md)). The classic detector is one click away |
| Missing angles on ZMQ | Bridge sent `0.0` | Same. The UDP and JSONL measurements use `null` instead |
| Configuration | ROS parameters and launch files | One commented TOML per rig, edited by dragging |
| Head and abdomen | Tracked | Tracked with the centroid method, off by default |

Rekkefly does not yet estimate wingbeat frequency or detect when flight stops.

Rekkefly was written from scratch after studying Kinefly's documentation and
`tracker_edge.py`. It keeps Kinefly's annular-sector and angular-gradient
approach and its angle conventions; it shares no code with it.

If you depend on a Kinefly feature that Rekkefly lacks,
[open a feature request](https://github.com/pvnkmrksk/rekkefly/issues/new/choose).
