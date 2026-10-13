# Changelog

All notable changes to Rekkefly are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/) (before 1.0, minor versions may
change behaviour; such changes are listed under **Changed**).

## [Unreleased]

### Added
- A website at <https://pvnkmrksk.github.io/rekkefly/>: an animated landing page
  that explains what Rekkefly measures, finds the right download for your
  computer, and links to the full documentation.
- A documentation book (mdBook), with search: installation per system, marker
  placement, the Bogong moth example, how the detectors work, real-footage and
  synthetic evidence, multi-rig labs, recording, output formats, the MCP server,
  complete configuration and command-line references, troubleshooting, a guide
  for Kinefly users, and development notes.
- Contributor guide, security policy, code of conduct, issue and pull request
  templates, and Dependabot updates for Cargo and GitHub Actions.
- `tests/docs.rs` keeps the documented command-line help and MCP tool list in
  sync with the program.

### Changed
- Links point to the renamed repository, `pvnkmrksk/rekkefly`, and the website.
- The website shows real footage instead of animations: the Bogong moth as
  filmed, with Rekkefly's tracking drawn on and its measurement of every frame,
  and the moth with painted antennae, classic detector beside Rekkefly
  (`scripts/web_media.py` makes both from the example video).
- Continuous integration is fast for everyday changes. Pull requests and
  pushes run formatting, lints and tests (Linux, plus tests on Windows and
  macOS) in a few minutes; website-only changes build only the website. The
  four-platform release build and installers run for `v*` tags, manual runs,
  and changes to packaging.

### Fixed
- The launchers in the Windows portable ZIP (**Launch Demo**, **Launch All
  Rigs** and the others) ran `rekke.exe`, which no longer exists. They now run
  `rekkefly.exe`. The installer was not affected.

## [0.6.0] - 2026-10-13

**Rekke is now Rekkefly, the new Kinefly.** Settings from Rekke 0.5 are copied
over on first launch.

### Added
- A robust default detector (`method = "wing_blur"`) that ignores legs,
  antennae and narrow lines: narrow features are removed, the median is taken
  across radial bands, and edges are ranked by the wing-blur shoulder behind
  them. On the real Bogong moth, painted antennae moved L − R by more than 5° on
  3 of 900 frames (classic detector: 489).
- The **Bogong moth example**: 30 s of a real tethered moth with measured hinge
  positions, shipped with every download and used when no camera opens.
- `rekkefly mcp`, a Model Context Protocol server with 15 tools, and `--json`
  output for scripts.
- `rekkefly track` for fast offline tracking (about 1,000 frames/s) with CSV
  and JSONL output, and `rekkefly clip` for cutting and shrinking videos.
- Native AVI playback (MJPEG, PNG, raw, uncompressed), including recordings cut
  short by a crash.
- Multi-rig manifests with per-rig `label`, `device`, `video`, ports and
  `record`; one window per config; a **New window** button.
- Run-only overrides: `--device`, `--video`, `--label`, `--zmq-port`,
  `--udp-port`, `--udp-target`.
- Debug traces with L − R, the filtered angular profile and a labelled edge.

### Changed
- MJPEG is the default recording format (about 10× smaller than raw).
- `primary_edge = "toward_head"` chooses the leading edge by polarity.
- New configs use wider sectors (−45° to +85° around each wing).
- In the GUI, a busy output port turns off only that output, with a notice.

### Fixed
- Linux `device = "auto"` failed while `"0"` worked: metadata-only
  `/dev/video*` nodes are now skipped and cameras are tried in order.
- Cameras without the requested size now open in the nearest mode.
- **Record**, `--record` and `--demo` no longer change the saved config.
- `--rigs … --demo` passed conflicting arguments to each rig.
- `rekkefly clip` from MP4/M4V kept only about half of the frames.

## [0.5.1] - 2026-10-12

### Fixed
- The Windows build links the Visual C++ runtime statically, so no separate
  runtime installation is needed.

## [0.5.0] - 2026-10-12

First release as **Rekke** (ರೆಕ್ಕೆ, Kannada for wing); earlier development
versions were called Wingtrace.

### Added
- Native webcam capture on macOS, Windows and Linux, with installers.
- Draggable, mirrored tracking sectors; markers and background saved
  automatically.
- Kinefly Docker-compatible ZMQ output, full UDP measurements, a multi-rig
  launcher, lossless grayscale recording, and a live plot.

[Unreleased]: https://github.com/pvnkmrksk/rekkefly/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/pvnkmrksk/rekkefly/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/pvnkmrksk/rekkefly/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/pvnkmrksk/rekkefly/releases/tag/v0.5.0
