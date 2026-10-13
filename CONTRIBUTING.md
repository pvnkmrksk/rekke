# Contributing to Rekkefly

Thank you for helping. Rekkefly is used in running experiments, so the bar is
simple: **never regress tracking, never lose data, and say what was checked.**

- **Questions and ideas:** open a [discussion or issue](https://github.com/pvnkmrksk/rekkefly/issues/new/choose).
- **Bugs:** use the bug report form. Include `rekkefly --version`, your system,
  and, if you can, a short clip (`rekkefly clip in.avi out.avi --duration 5`)
  and the rig's config. A clip that shows the problem is worth more than any description.
- **Security problems:** report them privately; see [SECURITY.md](https://github.com/pvnkmrksk/rekkefly/blob/main/SECURITY.md).
- **Documentation:** every page of the [docs](https://pvnkmrksk.github.io/rekkefly/docs/)
  has an edit button that opens the page's source in `book/src/`.

## Set up

Install [Rust](https://rustup.rs/) (stable) and the system packages listed in
[Building from source](https://pvnkmrksk.github.io/rekkefly/docs/development/building.html). Then:

```sh
git clone https://github.com/pvnkmrksk/rekkefly.git && cd rekkefly
cargo run --release -- samples/bogong-moth.avi --loop
```

## Before every commit

```sh
cargo fmt
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --no-default-features -- -D warnings
cargo test --locked
cargo test --locked --no-default-features
```

CI runs the same checks on every pull request in a few minutes: formatting and
lints on Linux, and the tests on Linux, Windows and macOS. Changes to only the
website or docs skip them and just build the site. The slow part, release
builds and installers for every platform, runs only for releases and for
changes to the packaging.

## House rules

- **Never regress tracking.** Detector changes must keep the synthetic tables
  (`cargo test --release --test intrusions -- --ignored --nocapture`) and the
  real-footage tables (`cargo test --release --test moth -- --ignored --nocapture`)
  at least as good, and keep the fast tests green. Put the before and after
  tables in the pull request.
- **Measure speed.** Tracking runs on the capture thread: avoid allocations and
  whole-frame copies there. Compare `rekkefly --benchmark` before and after.
- **Never lose user data.** Config writes are atomic and keep comments.
  Recordings never drop frames silently. Run-only overrides are never saved.
- **Fail loudly in experiments, gently in the GUI.** A missing camera or a busy
  port is an error headless and in rigs; in a double-clicked window it falls back
  with a visible banner.
- **Figures come from code.** Regenerate README and docs figures with the
  scripts and ignored tests instead of editing images.
- **Keep the docs in sync.** User-visible changes update the
  [book](https://github.com/pvnkmrksk/rekkefly/tree/main/book/src) and [CHANGELOG.md](https://github.com/pvnkmrksk/rekkefly/blob/main/CHANGELOG.md). `tests/docs.rs` fails when
  the command-line help or the MCP tool list drifts from the docs; refresh the
  help text with `REKKEFLY_BLESS=1 cargo test --test docs`.

## Documentation and website

The website has two parts, both deployed by `.github/workflows/pages.yml`:

| Part | Source | Built with |
| --- | --- | --- |
| Landing page | `site/` (plain HTML, CSS and JavaScript, no build step) | nothing |
| Documentation | `book/` | [mdBook](https://rust-lang.github.io/mdBook/) |

Preview both locally:

```sh
cargo install mdbook --locked --version 0.5.4
python3 scripts/build_site.py _site      # landing page + book + images
python3 -m http.server -d _site 8000     # open http://localhost:8000/
mdbook serve book --open                 # the book alone, reloading on save
```

The book's `images/` and `media/` folders are symbolic links to `docs/`. On
Windows, clone with `git clone -c core.symlinks=true` (with Developer Mode on)
or build the site in WSL.

## Pull requests

- Keep each pull request to one change, and explain *why* as well as what.
- Add a test for every bug fix. Tests that need real footage go in `tests/moth.rs`;
  synthetic ground truth in `tests/intrusions.rs` and `tests/tracking.rs`.
- Commit messages: a short imperative summary line, a blank line, then the reasons.

## Releasing (maintainers)

1. Update the version in `Cargo.toml`, `CITATION.cff`, `CHANGELOG.md` and
   `RELEASE_NOTES.md` (the release's notes), and merge to `main`. To try the
   installers first, start **Actions → Build and release → Run workflow** on
   `main` and download its artifacts.
2. Tag the merge commit and push the tag:
   `git tag -a v0.7.0 -m "Rekkefly 0.7.0" && git push origin v0.7.0`.
3. The build workflow (about 20 minutes) tests every platform, builds and
   smoke-tests the installers, then publishes them with `SHA256SUMS.txt`. Do
   not create the release by hand first.

By contributing you agree that your contribution is licensed under the
[MIT License](https://github.com/pvnkmrksk/rekkefly/blob/main/LICENSE), and to follow the [Code of Conduct](https://github.com/pvnkmrksk/rekkefly/blob/main/CODE_OF_CONDUCT.md).
