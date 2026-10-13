# Command line

```text
rekkefly                               open the window with the first free rig config
rekkefly VIDEO [--loop] [--fast]       track a video file
rekkefly --config FILE                 a specific rig config
rekkefly --rigs [rigs.toml]            start every rig in a manifest
rekkefly --list-cameras [--json]       show camera numbers, names and paths
rekkefly track VIDEO [options]         track a whole video as fast as possible
rekkefly clip IN OUT [options]         cut or convert a video
rekkefly mcp [--config-dir DIR]        Model Context Protocol server on stdin/stdout
```

On a Mac, use `/Applications/Rekkefly.app/Contents/MacOS/rekkefly` in place of
`rekkefly`, or [add a link](../getting-started/install.md#mac).

## Which config is used

1. `--config FILE`, if given.
2. Otherwise the first rig config not already open in another window:
   `camera-a.toml`, then `camera-b.toml`, then a new `camera-c.toml` and so on,
   in the folder Rekkefly uses for configs ([Files and folders](files.md)).

## Run-only overrides

These change the run, never the saved config:

| Option | Overrides |
| --- | --- |
| `--device SEL` | The camera |
| `--video FILE` (or the `VIDEO` argument) | Track a video instead of the camera |
| `--demo` | Use the simulated fly |
| `--label NAME` | The window title, instance name and recording prefix |
| `--zmq-port P` | The ZMQ publisher port |
| `--udp-port P` | The local UDP sending port |
| `--udp-target IP:PORT` | The UDP receiver |
| `--record` | Record from the start |

## Exit status

0 on success; non-zero, with the reason on standard error, on any error. In
headless runs a missing camera, a busy port or an invalid config is an error.

## Environment variables

| Variable | Effect |
| --- | --- |
| `REKKEFLY_FFMPEG` | Path to the FFmpeg program, for formats other than AVI and for network streams. Default: an `ffmpeg` beside Rekkefly, then on `PATH` |
| `REKKEFLY_SAMPLE` | A video to play instead of the Bogong moth when no camera opens |

The Rekke 0.5 names, `REKKE_FFMPEG` and `REKKE_SAMPLE`, still work.

## Full help

The complete output of `rekkefly --help` and of each subcommand's `--help`.
`tests/docs.rs` keeps this text identical to the program's.

```text
{{#include cli-help.txt}}
```
