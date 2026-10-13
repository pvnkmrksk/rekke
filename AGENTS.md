# Notes for AI agents

Two kinds of agent work with Rekkefly. Some **operate rigs**: tracking, measuring
and recording. Others **change the code**. Both are covered here.

## Operating Rekkefly

Use the MCP server: `rekkefly mcp` (setup and tools in [docs/mcp.md](docs/mcp.md)).
In a clone, the project `.mcp.json` starts it from source with `configs/`.
Without MCP, every operation has a scriptable CLI form with `--json` output:

```sh
rekkefly --list-cameras --json
rekkefly track samples/bogong-moth.avi --csv out.csv --json
rekkefly --config configs/demo.toml --headless --frames 300 --json
rekkefly --benchmark --json
```

Check marker placement by looking, not by guessing. `snapshot` (MCP), or
`rekkefly <video> --screenshot out.png` with a display, shows the sectors and
the edges that were found. Hinges belong on the wing bases. The inner radius
should sit just past the body, and the outer radius should end inside the wing blur.

## Changing the code

Rust 2024, one crate (`rekkefly`, library and binary). Before every commit:

```sh
cargo fmt
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --no-default-features -- -D warnings
cargo test --locked                       # CI runs debug tests on 4 platforms
cargo test --locked --no-default-features
```

CI (`.github/workflows/build.yml`) also builds release packages and runs smoke
tests on Ubuntu, Windows and both Mac architectures. A `v*` tag publishes a
release with `RELEASE_NOTES.md` as its notes.

| Area | Where | Checked by |
| --- | --- | --- |
| Wing detectors | `src/tracker.rs` | `tests/tracking.rs`, `tests/intrusions.rs` (synthetic ground truth), `tests/moth.rs` (real footage) |
| Sources, video, fallback | `src/source.rs`, `src/avi.rs`, `src/camera.rs`, `src/engine.rs` | `tests/sources.rs`, unit tests |
| Configs and run-only overrides | `src/config.rs`, `src/installation.rs`, `src/rigs.rs` | `tests/rigs.rs`, `tests/instances.rs`, `tests/persistence.rs` |
| MCP server | `src/mcp.rs` | `tests/mcp.rs` drives the real binary over stdio |
| Offline tracking, overlays | `src/analyze.rs`, `src/overlay.rs` | `tests/mcp.rs`, unit tests |
| GUI | `src/gui.rs` | egui event tests in the module; screenshots under Xvfb |
| Website and docs | `site/` (landing page), `book/` (mdBook) | `tests/docs.rs` (CLI help, MCP tools, config keys), `scripts/build_site.py` (links) |

House rules:

- **Never regress tracking.** Detector changes must keep the synthetic tables
  (`cargo test --release --test intrusions -- --ignored --nocapture`) and the
  real-footage tables (`cargo test --release --test moth -- --ignored --nocapture`)
  at least as good, and keep the fast tests green.
- **Measure speed.** `rekkefly --benchmark` and
  `cargo test --release --test moth throughput -- --ignored --nocapture`.
  Tracking runs on the capture thread; avoid allocations and whole-frame copies there.
- **Never lose user data.** Config writes are atomic and keep comments.
  Recordings never drop frames silently. Run-only overrides and the example's
  markers are never saved.
- **Fail loudly in experiments, gently in the GUI.** A missing camera or a busy
  port is an error headless and in rigs. In a double-clicked window it falls
  back (sample video, outputs off) with a visible banner.
- The README's figures come from scripts and ignored tests; regenerate them
  rather than editing images (see "Regenerating assets" in the README).
- User-visible changes update the docs in `book/src/` and `CHANGELOG.md`. After
  changing CLI help or an MCP tool, run `REKKEFLY_BLESS=1 cargo test --test docs`.
