# Tests

Over 100 tests run on every pull request, on Ubuntu, Windows, and macOS on
Apple silicon and Intel, with and without the GUI and camera features. CI then
builds each installer, installs it and runs it.

```sh
cargo test --locked                      # everything fast, with the GUI and cameras
cargo test --locked --no-default-features
```

## What is tested where

| Area | Tests |
| --- | --- |
| Wing detectors | `tests/tracking.rs`, `tests/intrusions.rs` (synthetic flies with known angles), `tests/moth.rs` (real footage) |
| Geometry and marker editing | `tests/geometry.rs` |
| Sources, video, fallback, subscriptions | `tests/sources.rs`, unit tests in `src/avi.rs` and `src/source.rs` |
| Configs, run-only overrides, rigs | `tests/rigs.rs`, `tests/instances.rs`, `tests/persistence.rs` |
| ZMQ compatibility | `tests/zmq.rs` |
| MCP server | `tests/mcp.rs` drives the real binary over stdio |
| Documentation | `tests/docs.rs`: the CLI help, MCP tools and config keys documented here match the program |
| GUI | egui event tests in `src/gui.rs` |

## Report-only tests

Ignored tests print the comparison tables and produce the figures in these docs:

```sh
cargo test --release --test intrusions -- --ignored --nocapture   # synthetic tables
cargo test --release --test moth -- --ignored --nocapture         # real-footage tables
cargo test --release --test moth throughput -- --ignored --nocapture
```

Detector changes must keep both tables at least as good. Put them, before and
after, in the pull request.

## Documentation tests

`tests/docs.rs` fails when the docs drift from the program:

- `book/src/reference/cli-help.txt` must equal the program's `--help` output;
- `docs/mcp.md` must name every MCP tool, and `book/src/reference/mcp-tools.md`
  must equal the server's tool list;
- [Configuration](../reference/configuration.md) must name every key of the default config.

After changing the help text or a tool, regenerate the files:

```sh
REKKEFLY_BLESS=1 cargo test --test docs
```

The website build (`scripts/build_site.py`) also checks every link and image in
the built site.
