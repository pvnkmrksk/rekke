# Rekkefly

<p class="lede"><strong>Rekkefly is the new Kinefly.</strong> It tracks the wings of a tethered insect (a fly, bee or moth) from a camera or a video file. It streams each wing's angle and <strong>L − R</strong>, the steering signal, to your closed-loop experiment in real time. It is one small native program for macOS, Windows and Linux, with no ROS, Python, OpenCV or Docker to install.</p>

<div class="hero-media">
<video poster="media/moth-tracking.jpg" autoplay muted loop playsinline aria-label="Real footage of a tethered Bogong moth with Rekkefly's wing sectors and reported leading edges drawn on top">
<source src="media/moth-tracking.mp4" type="video/mp4">
<source src="media/moth-tracking.webm" type="video/webm">
</video>
</div>

*A real tethered Bogong moth, the example that ships with every download,
exactly as filmed at 30 fps. Drawn on top: the body axis, each wing's sector
around its hinge, and the leading edge Rekkefly reports (bold) with the other
edge it found (thin). The window, with its debug traces, is shown in
[The window](guide/app.md).*

## What it does

- **Measures wing-stroke amplitude** the way Kinefly does: it finds the leading
  edge of each wing's motion-blur envelope inside an annular sector around the
  wing hinge, every frame.
- **Ignores legs and antennae.** On the real moth, thin lines painted through the
  wings moved Rekkefly's L − R on 3 of 900 frames, and the classic detector's on
  489 ([evidence](tracking/evidence.md)).
- **Keeps up easily.** About 340 µs per 640×480 frame. A 30 fps rig uses 6% of
  one core and 10 MB of memory ([performance](tracking/performance.md)).
- **Speaks Kinefly.** The ZMQ message is the one the Kinefly Docker bridge sends,
  so existing subscribers keep working ([outputs](guide/outputs.md)).
- **Runs a lab, not just a rig.** One TOML file starts every rig with its own
  camera, ports, label and settings ([several rigs](guide/rigs.md)).
- **Works with agents and scripts.** A built-in [MCP server](guide/agents.md)
  lets an AI agent run rigs, look at the tracking and measure trials, and every
  command has `--json` output.

## Where to start

| You want to | Read |
| --- | --- |
| Install it and see it track | [Install](getting-started/install.md), then [Quick start](getting-started/quick-start.md) |
| Set up a new rig well | [Placing the markers](getting-started/markers.md) and [Tuning a rig](tracking/tuning.md) |
| Move an existing Kinefly setup | [Coming from Kinefly](kinefly.md) |
| Connect your experiment | [Receiving measurements](guide/outputs.md) |
| Run many arenas | [Several rigs at once](guide/rigs.md) |
| Let an AI agent run it | [AI agents (MCP)](guide/agents.md) |
| Know how far to trust it | [The detectors](tracking/detectors.md) and [Evidence](tracking/evidence.md) |
| Look up a setting | [Configuration](reference/configuration.md) and [Command line](reference/cli.md) |

## The name

Rekkefly joins **ರೆಕ್ಕೆ** (*rekke*, the Kannada word for wing) with **fly**. It
nods to its roots in [Kinefly](https://github.com/ssafarik/Kinefly), which made
real-time wing tracking practical for tethered-flight rigs, and
[Kinefly Docker](https://github.com/pvnkmrksk/Kinefly_Docker), which kept it
running. Rekkefly was written from scratch in Rust; it shares no code with either.

Rekkefly is free software under the [MIT License](citing.md). The source,
releases and issue tracker are on [GitHub](https://github.com/pvnkmrksk/rekkefly).
