# Questions and answers

### Which insects does Rekkefly work with?

Any tethered insect whose beating wings leave a motion-blur envelope in the
image: flies (*Drosophila* and larger), bees, moths. The example that ships with
it is a Bogong moth, and the synthetic test flies have six legs, antennae and
translucent wings.

### Does it need a particular camera?

No. Any camera your system can use as a webcam works: USB machine-vision
cameras with UVC support, webcams, and built-in cameras. For wing-stroke
amplitude, the exposure should be long enough to blur several wingbeats into an
envelope. Video files and network streams work too.

### How fast is it? Can it keep up with a high-speed camera?

Tracking takes about 0.3 ms per 640×480 frame, so one rig can process about
3,000 frames/s of tracking alone, and about 1,000 frames/s when it also has to
decode MJPEG video. See [Performance](tracking/performance.md).

### What is the latency?

A frame is tracked on the capture thread the moment it arrives, and its
measurement is published immediately: the added delay is the tracking time
(about 0.3 ms) plus sending a small message. The camera's own exposure and
transfer usually dominate.

### Can I use it with my existing Kinefly experiment code?

Yes. Rekkefly publishes the same ZMQ JSON message as the Kinefly Docker bridge,
on the same default port. See [Coming from Kinefly](kinefly.md).

### Does it measure wingbeat frequency?

Not yet. Rekkefly measures stroke amplitude (the envelope's leading edge), as
Kinefly does. Wingbeat frequency and stopped-flight detection are not implemented.

### Is it accurate?

On synthetic flies with known angles, the median error is about 0.25°, and no
frame was off by more than 4° even with legs kicking through the wings. On the
real Bogong moth, which has no ground truth, painted antennae moved L − R on 3 of
900 frames. See [Evidence](tracking/evidence.md), including what has not been
verified yet.

### Why "Rekkefly"?

ರೆಕ್ಕೆ (*rekke*) is Kannada for wing. Rekkefly is the wing-tracker for flies,
and the successor to Kinefly.

### Is it free?

Yes: free and open source under the MIT License, for any use, including
commercial. If you use it in published research, please [cite it](citing.md).

### Can an AI agent run my rigs?

Yes, through the built-in [MCP server](guide/agents.md). It can start and stop
rigs, look at the tracking overlay, move markers, record and measure trials.
Agents only get the access you give the MCP client that runs it.

### Does Rekkefly send data anywhere?

Only where you configure it: ZMQ and UDP measurements to the addresses in the
config. It has no telemetry and checks for nothing online.

### Can I run it without a screen?

Yes: `--headless`, or rigs from a manifest with `--rigs … --headless`. A build
without the GUI and camera support is also possible
(`cargo build --release --no-default-features`) for offline analysis on servers.
