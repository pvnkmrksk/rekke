# Tests

Over 100 tests run on every pull request and push to main, on Linux, Windows
and macOS, in a few minutes (`.github/workflows/ci.yml`). Linux also runs them
without the GUI and camera features, and checks formatting and lints.

| Workflow | Runs for | Does |
| --- | --- | --- |
| `ci.yml` | Pull requests and pushes that change code | Formatting, lints, tests on Linux, Windows and macOS |
| `pages.yml` | Every pull request and push to main | Builds the website, checks every link, publishes it from main |
| `build.yml` | `v*` tags, manual runs, pull requests that change the packaging scripts, icons or workflow | Release builds, installers and smoke tests on Ubuntu, Windows and both Mac architectures; a tag publishes the release |

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
| ZMQ compatibility | `tests/zmq.rs`: a ZMTP 3 subscriber (zmq.rs), a replayed ZMTP 2.0 NetMQ handshake, topic filtering and stalled subscribers; unit tests in `src/zmq_output.rs` check the message bytes against Python's `json.dumps` |
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
