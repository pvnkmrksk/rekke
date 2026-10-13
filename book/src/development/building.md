# Building from source

Rekkefly is one Rust crate, `rekkefly`, with a library and a binary.

## Requirements

Install [Rust](https://rustup.rs/) (stable; the crate uses the 2024 edition), then:

| System | Also install |
| --- | --- |
| Ubuntu / Debian | The packages below |
| macOS | Xcode command-line tools (`xcode-select --install`); on Intel Macs also NASM (`brew install nasm`) |
| Windows | Visual Studio Build Tools (MSVC) and [NASM](https://www.nasm.us/) on `PATH` |

```sh
sudo apt-get install build-essential pkg-config nasm libclang-dev libv4l-dev libx11-dev libxi-dev \
  libxrandr-dev libxcursor-dev libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev libegl1-mesa-dev
```

## Build and run

```sh
git clone https://github.com/pvnkmrksk/rekkefly.git && cd rekkefly
cargo run --release -- samples/bogong-moth.avi --loop   # try it
cargo build --release --locked                          # target/release/rekkefly
```

Release builds are optimized; debug builds are fine for development (image
codecs are optimized even in debug builds, so the tests on real video stay fast).

## Features

| Feature | Default | Adds |
| --- | --- | --- |
| `gui` | On | The desktop window (eframe/egui) |
| `camera` | On | Native camera capture (nokhwa; v4l on Linux) |

Without both, Rekkefly is a processing-only program for servers and offline
analysis: `track`, `clip`, `mcp` with videos and the simulated fly, headless runs
and benchmarks.

```sh
cargo build --release --locked --no-default-features
```

## Packages

`scripts/package.py` builds the installers from a release binary, as CI does:

```sh
python3 scripts/package.py --platform linux-x64 --binary target/release/rekkefly --out dist
```

| Platform | Produces | Needs |
| --- | --- | --- |
| `linux-x64` | `.deb`, tarball | `dpkg-deb` |
| `macos-arm64`, `macos-x64` | `.dmg` and portable ZIP | macOS, `pip install dmgbuild` |
| `windows-x64` | Installer `.exe` and portable ZIP | NSIS (`makensis`) |

## The website

The [website](https://pvnkmrksk.github.io/rekkefly/) is built from `site/` (the
landing page: plain HTML, CSS and JavaScript) and `book/` (this documentation,
built with [mdBook](https://rust-lang.github.io/mdBook/)):

```sh
cargo install mdbook --locked --version 0.5.4
python3 scripts/build_site.py _site       # landing page + book + images, checked for broken links
python3 -m http.server -d _site 8000      # open http://localhost:8000/
mdbook serve book --open                  # only the book, rebuilt on every save
```

`.github/workflows/pages.yml` builds the same site on every pull request and
deploys it from `main`.

## Assets

Figures are generated, not drawn by hand. Regenerate them rather than editing
images:

| Asset | Command |
| --- | --- |
| Icon, banners, social card, DMG background | `python3 scripts/brand.py`, then `python3 scripts/icons.py` (needs librsvg, ImageMagick, and the Inter and Noto Sans Kannada fonts) |
| Real-moth antenna chart | `REKKEFLY_MOTH_TRACE=moth.csv cargo test --release --test moth antenna_trace -- --ignored`, then `python3 scripts/moth_chart.py moth.csv docs/images/moth-antennae` |
| Throughput chart | `cargo test --release --test moth throughput -- --ignored --nocapture`, then `python3 scripts/perf_chart.py docs/images/throughput <1> <2> <4> <8>` |
| Synthetic L − R chart | `REKKEFLY_TRACE=trace.csv cargo test --release --test intrusions trace -- --ignored`, then `python3 scripts/trace_chart.py trace.csv docs/images/lr-trace` |
| Detector figure | `REKKEFLY_FIGURE=docs/images/legs-classic-vs-robust.png cargo test --release --test intrusions figure -- --ignored` |
| Website videos of the real moth, and their angles | `python3 scripts/web_media.py` (an ignored test draws the tracking on the raw frames; FFmpeg encodes them) |
| Screenshots, GIF and video of the window | Under Xvfb, with `--show plots --screenshot PNG --screenshot-after SECONDS` and `ffmpeg -f x11grab` |
