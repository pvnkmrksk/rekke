## What and why

<!-- What does this change, and why is it needed? Link the issue it fixes. -->

## How it was checked

- [ ] `cargo fmt`, and `cargo clippy --locked --all-targets -- -D warnings` with and without `--no-default-features`
- [ ] `cargo test --locked` and `cargo test --locked --no-default-features`
- [ ] Detector changes: the synthetic and real-footage tables are at least as good (paste before and after)
- [ ] Speed-sensitive changes: `rekkefly --benchmark` before and after
- [ ] User-visible changes: docs in `book/src/` and `CHANGELOG.md` updated
