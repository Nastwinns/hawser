# Contributing

Thanks for helping improve haw. This project stays small at the core; most
extensions belong in [plugins](docs/PLUGINS.md) and hooks.

## Build & test

Rust edition 2024. These must pass before every commit:

```bash
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

House rules (see [`.claude/rules/rust.md`](.claude/rules/rust.md) and the README):

- All domain logic lives in `haw-core`. `hawser` (the `haw` binary) and `haw-tui`
  stay thin: parse args, call core, render — no domain decisions in front-ends.
- Library crates use typed `thiserror` errors; `anyhow` only in binaries.
- No `unwrap()`/`expect()` in library code paths (tests may).
- Paths via `Path`/`PathBuf`, never hard-coded separators. `unsafe` is forbidden.
- Public API items get rustdoc (`///`); no inline explanatory comments.

## Pull requests

- One focused change per PR. Keep the diff reviewable.
- Include tests for new behavior; keep the suite green.
- Match the surrounding style. Run fmt + clippy before pushing.
- Write a clear title and a short description of what and why.

## Plugins

A plugin is an executable named `haw-<name>` on `PATH`; `haw <name>` dispatches to
it in a separate process. You don't need to modify this repo to build one — see
[docs/PLUGINS.md](docs/PLUGINS.md) for the full dispatch contract, the
`haw.plugin/1` context, and a worked `haw hello` example in shell and Rust. A
runnable example lives in [`examples/haw-hello`](examples/haw-hello).

To share a plugin with the community:

1. Make sure it follows the conventions in docs/PLUGINS.md (named `haw-<verb>`,
   self-describing `--help`, JSON on `--format json`, meaningful exit codes).
2. Publish it (a `haw-<name>` crate on crates.io, or a downloadable binary/script).
3. Open a PR adding one line to the community plugin list: name, a one-sentence
   description, and a link. Keep entries alphabetical.

We review plugin listings for a working link and a clear description — the plugin
itself lives in your repo, not this one.
