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

## Where to go next

- **[Architecture](ARCHITECTURE.md)** — crate layout, data flows, the phased plan.
- **[CLI design & TUI keymap](CLI-DESIGN.md)** — the full verb lexicon and the
  cockpit keymap.
- **[Extending](EXTENDING.md)** — plugins, hooks, auth, CI/CD integration.
- **[Compliance](COMPLIANCE.md)** — tool qualification, SBOM/CRA, signing, GDPR.
- **[Commercialization](COMMERCIALIZATION.md)** — editions, licensing, pricing, GTM.
- **[Launch playbook](LAUNCH.md)** — timing, assets, copy.

## Links

- **Repository:** <https://github.com/Nastwinns/keelson>
- **Try the TUI in your browser:** <https://nastwinns.github.io/keelson/>
- **README:** the [project README](https://github.com/Nastwinns/keelson#readme)
  has install instructions, demos, and the manifest walkthrough.
