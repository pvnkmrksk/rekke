**Rekkefly is the new Kinefly.** The name is ರೆಕ್ಕೆ (*rekke*, Kannada for wing) + fly, rooted in [Kinefly](https://github.com/ssafarik/Kinefly) and [Kinefly Docker](https://github.com/pvnkmrksk/Kinefly_Docker). It tracks the wings of tethered insects from cameras or video files, publishes Kinefly-compatible ZMQ, and needs no ROS, Python, OpenCV or Docker. Documentation: https://pvnkmrksk.github.io/rekkefly/docs/

## 0.6.1: ZMQ for every Kinefly subscriber, Unity included

**Fixed**
- **Unity received no ZMQ messages from Rekkefly 0.6.0**, while PlotJuggler and Python did. Many Unity projects bundle an older NetMQ (3.x or 4.0.0.x) that speaks only version 2.0 of ZeroMQ's wire protocol (ZMTP), and the ZeroMQ library inside Rekkefly 0.6.0 spoke only version 3. Rekkefly now negotiates ZMTP 3.0, 2.0 or 1.0 the way libzmq does, so every subscriber that worked with the Kinefly Docker bridge works with Rekkefly, with no change to the subscriber: an existing Unity build keeps working as it is. Checked with NetMQ 4.0.0.1 and 4.0.4.4, pyzmq (libzmq 4.3.5) and zmq.rs.
- **The ZMQ message is byte for byte the bridge's**, with the same key order, spacing and number format: `{"timestamp_unix": 1700000000.125, "pitch": 0.0, "y": -0.5, "x": 0.25, "yaw": 0.75, "z": 0.0, "roll": 0.0, "frame_number": 77}`.
- The launchers in the Windows portable ZIP (**Launch Demo**, **Launch All Rigs** and the others) ran `rekke.exe`, which no longer exists, and closed at once. They now run `rekkefly.exe`. The installer was not affected.

**Changed**
- A slow ZMQ subscriber no longer stops the run. As with libzmq, a subscriber more than 1,000 messages behind misses messages (counted as `zmq_dropped`), and tracking never waits. Subscriptions to a topic prefix are honoured.

**New**
- **A website**, https://pvnkmrksk.github.io/rekkefly/, shows real footage of the Bogong moth as Rekkefly tracks it and offers the right download for your computer. Its documentation covers installation, marker placement, how the detectors work and the evidence for them, multi-rig labs, output formats, the MCP server, every config key and command-line option, troubleshooting, and moving from Kinefly.

**Upgrading.** Install over 0.6.0. Configs, markers and recordings are unchanged, and so is everything on the receiving side: keep your Unity build, PlotJuggler layout or Python script as they are.

Downloads: `Rekkefly-0.6.1-macos-arm64.dmg`, `Rekkefly-0.6.1-macos-x64.dmg`, `Rekkefly-0.6.1-windows-x64-setup.exe`, `rekkefly_0.6.1_amd64.deb`, plus portable ZIPs and a Linux tarball. Checksums are in `SHA256SUMS.txt`.
