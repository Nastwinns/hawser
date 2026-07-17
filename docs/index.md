# hawser

**Pin a stack of Git repos to a lockfile — so a teammate, a CI runner, or an
auditor checks out the _identical_ tree, everywhere. One binary. In Rust.**

`haw` composes many independent Git repos into one reproducible stack. A declarative
manifest (`haw.toml`) describes **stacks** and the **repos** they compose; a committed
lockfile (`haw.lock`) pins every repo to an exact revision — so any teammate or CI
machine rebuilds the byte-identical tree. No submodules, no detached HEADs, no Python
runtime — one static binary.

```sh
haw init haw.toml   # declare the repos
haw sync            # clone every repo, write haw.lock (exact SHAs)
haw verify          # CI gate: exit 3 if the tree drifts from the lock
```

This is the reference documentation for **hawser 0.1.4**. New to the project? Start with
the [Learn course](learn/00-what-is-hawser.md) for a guided, hands-on path; use the pages below as
reference.

## Beyond the core

Reproducible **compose** (above) is the foundation. Built on it, four more capabilities —
one binary:

- **Compose, at scale.** Stacks and overlays compose repos into named variants. Shallow
  (`--depth`) and partial (`--filter=blob:none`) clone, plus a shared object store via
  git `alternates` (`--shared`) — no symlinks, so it runs on Windows.
- **Orchestrate.** Run `build`, `test`, or any command across the whole fleet in
  parallel (`-j N`); `haw grep` fans out across every repo; `haw verify` is a drift gate
  that exits 3 for CI.
- **Collaborate.** One feature = one branch across N repos, with cross-linked PR/MRs on
  **GitHub, GitLab, and Bitbucket**, an aggregated review + CI status, and `land` to
  merge in dependency order. Plus a parallel collaborative merge (`plan` → `resolve` →
  `cleanup`).
- **Operate.** A k9s-style TUI cockpit (bare `haw`) — a live fleet grid you drill into
  for a repo's diff, a PR's checks, or a CI run's live progress, then act from the
  keyboard. Fuzzy filter, marks + bulk actions, a problems-only view, drift highlights,
  a command bar, and six themes.
- **Govern.** SBOM (CycloneDX + SPDX), SLSA/in-toto provenance, cosign/minisign
  signatures, lifecycle **hooks**, a secret/hygiene **gate**, and `evidence` bundles for
  qualification.

## Extend it

`haw <name>` runs `haw-<name>` from your `PATH` — extend the CLI without forking. Plugins
are subprocesses speaking a JSON contract (`haw.plugin/1` in, `haw.plugin.report/1` out),
so they can be written in **Rust, Python, Go, or shell**. Scaffold, discover, and install
them:

```bash
haw plugins new my-check --lang python   # runnable skeleton
haw plugins list --remote                # discover community plugins from the index
haw plugins install aspice               # install a first-party or community plugin
```

Published [JSON Schemas](https://github.com/Nastwinns/hawser/tree/main/schemas) and thin
[bindings](https://github.com/Nastwinns/hawser/tree/main/bindings) (Python, Go) make it
trivial; see the curated
[AWESOME-HAW-PLUGINS](https://github.com/Nastwinns/hawser/blob/main/AWESOME-HAW-PLUGINS.md)
list.

## Distribute & install

`haw` ships as a single static binary. Install via crates.io, Homebrew, Scoop, AUR, Nix,
`.deb`/`.rpm`, Docker, or a signed prebuilt archive — every release carries a `.sha256`
checksum and a keyless **cosign** signature. Fleet artifacts publish to private registries
(Nexus, Artifactory, GitLab, Bitbucket) via `haw publish`.

```sh
cargo install hawser         # from crates.io (canonical)
brew install nastwinns/tap/hawser   # macOS / Linux (Homebrew)
curl -L … | tar xz           # prebuilt static musl binary
```

## Security

`haw` runs code declared in the manifest and binaries on your `PATH` — treat both as
trusted inputs. The crate is `#![forbid(unsafe_code)]`, HTTPS is rustls-only, actions are
SHA-pinned, releases are cosign-signed, and `cargo audit`/`cargo deny` gate every push.
Read the full **[trust model](SECURITY.md)**.

## Try it in your browser

The landing page renders the live cockpit TUI in WebAssembly — no install required:
<https://nastwinns.github.io/hawser/>

## Where to go next

- **[Learn](learn/00-what-is-hawser.md)** — the guided, hands-on course from zero to a governed fleet.
- **[Install](INSTALL.md)** — the full channel matrix + signature verification + air-gap.
- **[Distribution](DISTRIBUTION.md)** — mirror releases to Nexus/Artifactory/GitLab/Bitbucket.
- **[Domains](DOMAINS.md)** — how the loop maps onto embedded, microservices, ML, infra, mobile.
- **[Architecture](ARCHITECTURE.md)** — crate layout, concurrency model, forge abstraction.
- **[CLI design & TUI keymap](CLI-DESIGN.md)** — the full verb lexicon and cockpit keymap.
- **[Extending](EXTENDING.md)** — plugins, hooks, auth, and CI/CD integration.
- **[Plugins](PLUGINS.md)** — writing and submitting `haw-<name>` plugins.
- **[Compliance](COMPLIANCE.md)** — tool qualification, SBOM/CRA, signing, GDPR.
- **[Security](SECURITY.md)** — the trust model: what `haw` executes, plugin trust, tokens.

## Links

- **Repository:** <https://github.com/Nastwinns/hawser>
- **Try the TUI in your browser:** <https://nastwinns.github.io/hawser/>
- **README:** the [project README](https://github.com/Nastwinns/hawser#readme) has the
  install matrix, demos, and the manifest walkthrough.
