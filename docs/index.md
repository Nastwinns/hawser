# hawser

**Reproducible multi-repo stacks + cross-repo PR/MR orchestration. One binary, one TUI. In Rust.**

`haw` assembles a software stack out of many independent Git repositories — without
submodules, without detached HEADs, without a Python runtime. A declarative manifest
(`haw.toml`) describes **stacks** and the **repos** they are composed of; a committed
lockfile (`haw.lock`) pins every repo to an exact revision, so any teammate or CI
machine reproduces the exact same tree.

On top of composition, `haw` drives the day-to-day multi-repo workflow: branch a
feature across all affected repos at once, open the linked PRs/MRs on GitHub **and**
GitLab, and track review + CI state from one keyboard-driven cockpit.

## Why haw

- **Reproducible by default.** The committed `haw.lock` pins every repo to an exact
  revision. `haw sync` reconstructs the identical tree on any machine — no drift,
  no "works on my laptop."
- **No submodules, no Python.** A single static binary. No `.gitmodules` foot-guns,
  no detached HEADs, no runtime to install on CI.
- **One changeset, many repos.** Start a feature across every affected repo, open
  the linked PRs/MRs on GitHub and GitLab, and watch review + CI land — from one
  keyboard-driven TUI.
- **Honest scope.** Git mutations shell out to `git`; reads go through gitoxide.
  Formats and forges sit behind traits, so new manifest formats or forges are just
  new implementations.

## Install

```sh
brew install hawser          # macOS / Linux (Homebrew)
cargo install hawser         # from crates.io
curl -L … | tar xz           # prebuilt static musl binary
```

See the [project README](https://github.com/Nastwinns/keelson#readme) for the full
install matrix, a manifest walkthrough, and recorded demos.

## Try it in your browser

The landing page renders the live cockpit TUI in WebAssembly — no install required:
<https://nastwinns.github.io/keelson/>

## Where to go next

- **[Architecture](ARCHITECTURE.md)** — crate layout (`haw-core`, `hawser`,
  `haw-tui`), data flows, and the phased plan.
- **[CLI design & TUI keymap](CLI-DESIGN.md)** — the full verb lexicon and the
  cockpit keymap.
- **[Extending](EXTENDING.md)** — plugins, hooks, auth, and CI/CD integration.
- **[Plugin ecosystem](PLUGIN-ECOSYSTEM.md)** — the plugin study and roadmap.
- **[Production-fit validation](PROD-VALIDATION.md)** — how haw is validated for
  real-world use.
- **[Qualification kit](QUALIFICATION-KIT.md)** — the tool-qualification skeleton
  for regulated environments.
- **[Compliance](COMPLIANCE.md)** — tool qualification, SBOM/CRA, signing, GDPR.
- **[Commercialization](COMMERCIALIZATION.md)** — editions, licensing, pricing, GTM.
- **[Launch playbook](LAUNCH.md)** — timing, assets, copy.

## Links

- **Repository:** <https://github.com/Nastwinns/keelson>
- **Try the TUI in your browser:** <https://nastwinns.github.io/keelson/>
- **README:** the [project README](https://github.com/Nastwinns/keelson#readme)
  has install instructions, demos, and the manifest walkthrough.
