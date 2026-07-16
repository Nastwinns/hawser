<!-- markdownlint-disable MD033 MD041 -->
<div align="center">

<img src="docs/assets/hawser-comic.jpeg" alt="hawser — the beam that binds the repos" width="720">

# hawser

**Compose a software stack from many Git repos, pin it to a lockfile, and drive every
cross-repo PR, review, and CI run from one keyboard cockpit. One binary. In Rust.**

[![crates.io](https://img.shields.io/crates/v/hawser)](https://crates.io/crates/hawser)
[![CI](https://github.com/Nastwinns/hawser/actions/workflows/ci.yml/badge.svg)](https://github.com/Nastwinns/hawser/actions/workflows/ci.yml)
[![rust](https://img.shields.io/badge/rust-1.90%2B-orange?logo=rust)](https://www.rust-lang.org)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![unsafe](https://img.shields.io/badge/unsafe-forbidden-success.svg)](Cargo.toml)
[![docs](https://img.shields.io/badge/docs-github%20pages-8A2BE2)](https://nastwinns.github.io/hawser/docs/)

[Install](#install) · [Quick start](#quick-start) · [The cockpit](#the-tui-cockpit) ·
[Docs](https://nastwinns.github.io/hawser/docs/) ·
[Try the TUI in your browser](https://nastwinns.github.io/hawser/)

</div>

---

`git` is great for one repository. `haw` is for the ten you ship together.

A declarative manifest (`haw.toml`) lists your repos and composes them into named
**stacks**. A committed lockfile (`haw.lock`) pins every repo to an exact SHA — so a
teammate, or a CI runner, or an auditor rebuilds the *identical* tree, byte for byte.

On top of that reproducible base, `haw` runs the daily multi-repo loop: branch one
feature across every affected repo, open the linked PRs/MRs on GitHub **and** GitLab,
then review, merge, and land them — all from a k9s-style cockpit that never leaves the
terminal.

No submodules. No detached HEADs. No symlinks. No Python.

![haw TUI cockpit](demo/haw-tui.gif)

## Why it exists

Splitting a product across repositories is routine — shared HAL/BSP repos reused
across ECUs in automotive and avionics, or a fleet of backend microservices. But the
tooling forces a choice, and every option gives up something:

| Tool | Gives you | Gives up |
|------|-----------|----------|
| Google `repo` | manifest-driven multi-repo checkout | a lockfile; **symlinks the git internals** (breaks on Windows); detached HEADs; needs Python |
| Zephyr `west` | manifest + per-project update | reproducible pinning; detached HEADs; needs Python |
| RepoFleet (Go) | issue → branches → PR/MR flow | stack composition; reproducible pinning |
| mergetopus (Rust) | parallel single-repo merges | anything multi-repo |

`haw` is the union nobody ships: **reproducible composition** + **cross-repo PR/MR
orchestration** + optional **parallel collaborative merge**, behind one static binary
and one TUI. It orchestrates Git and the forge APIs — it does not reimplement Git's
merge engine, replace a forge, or replace your toolchain.

> **Why no symlinks, when `repo` needs them?** `repo` shares one object store across
> hundreds of Android repos and wires each checkout to it with symlinks — a hard
> requirement in 2008, before `git worktree` and partial clone existed. `haw` clones
> each repo as a plain, autonomous git repo (disk is cheap now), and gets reproducible
> builds from `haw.lock`, not a shared store. Object sharing is *opt-in* via git's
> native `alternates` (`--shared`) — a text file, never a symlink. So `haw` runs
> unchanged on Windows.

## Highlights

- **Reproducible by default.** `haw.lock` pins every repo to a SHA. Runs are
  byte-identical across machines and OSes — a real argument in automotive/avionics
  audits, where the lockfile *is* the evidence.
- **Plain repos on disk.** Every repo is an ordinary, complete git clone you can `cd`
  into and use directly. No symlinks, no detached HEAD — so it works on Windows, where
  `repo` struggles.
- **Stacks compose, revs overlay.** Named repo sets share the same clones; overlays
  override revisions per variant without ever duplicating a repo list.
- **One feature, N repos.** A *changeset* opens one branch everywhere, cross-links the
  PR/MRs, aggregates their status, and `land`s them in dependency order.
- **A cockpit, not a dashboard.** Bare `haw` is a k9s-grade TUI: live refresh, fuzzy
  filter, sort, marks and bulk actions, cross-repo `grep`, a file browser (local *and*
  remote), a drop-to-shell, and drill-ins that show a repo's diff, a PR's checks, a CI
  run's live progress — then `merge`, `approve`, or `checkout`, all from the keyboard.
- **Loud about drift.** Any repo whose HEAD has drifted from the lock — or is dirty, or
  uncloned — turns red with a ⚠ marker; `p` filters the fleet to just the problems.
- **Fast and native.** Reads go through [gitoxide](https://github.com/GitoxideLabs/gitoxide);
  only heavy plumbing shells out to `git`. `sync`, `run`, `build`, `test` all run in
  parallel.
- **CI-friendly.** `haw verify` exits 3 on drift; `--json` where it matters;
  `NO_COLOR`/`CLICOLOR_FORCE` honored like `bat`, `eza`, and `ripgrep`.

## Install

Pick a package manager — all install the same `haw` binary:

```bash
cargo install hawser                                             # Rust / crates.io (canonical)
brew install nastwinns/tap/hawser                                # macOS + Linux (Homebrew)
scoop bucket add nastwinns https://github.com/Nastwinns/scoop-bucket && scoop install hawser   # Windows
```

**Static Linux binary (recommended for servers, containers, air-gap).** The musl build
is fully static — no glibc, no runtime — so one file runs on any Linux host:

```bash
curl -sSL https://github.com/Nastwinns/hawser/releases/download/v0.1.2/haw-0.1.2-x86_64-unknown-linux-musl.tar.gz \
  | tar xz && sudo install haw /usr/local/bin/
```

*Air-gapped host?* Download the archive plus its `.sha256`, `.sig`, and `.pem` on a
connected machine, verify them (below), copy all four files across, and install. The
static binary has no runtime dependencies, so nothing else needs to cross the gap.

**Signed releases.** Every platform — x86_64/aarch64 Linux (glibc), x86_64 musl
(static), x86_64/aarch64 macOS, x86_64 Windows — ships on the
[GitHub Release](https://github.com/Nastwinns/hawser/releases/latest) with a `.sha256`
checksum and a keyless **cosign** signature (`.sig`/`.pem`) you can verify offline.

Other channels — `.deb`/`.rpm`, AUR (`hawser-bin`), Nix flake, Docker, from source:

```bash
cargo install --git https://github.com/Nastwinns/hawser hawser   # latest main
nix run github:Nastwinns/hawser                                  # run once, no install
docker build -t haw . && docker run --rm haw --version           # container
```

Full channel matrix, signature verification, and the air-gap workflow:
**[docs/INSTALL.md](docs/INSTALL.md)**.

## Quick start

```bash
haw init examples/quickstart/haw.toml   # bootstrap from a ready-made example
haw sync                                # clone every repo, write haw.lock
haw                                     # open the cockpit
```

New here? [`examples/`](examples/) has runnable, copy-pasteable manifests to learn from.

A typical session — compose, inspect, branch across repos:

```console
$ haw tree
haw.toml
├─ gateway
│  ├─ kernel    v6.1.2       (git@gitlab.company.com:firmware/kernel.git)
│  ├─ hal       main         (git@gitlab.company.com:firmware/hal.git)
│  └─ app-mqtt  release/2.x  (git@github.com:acme/app-mqtt.git)
└─ sensor-node
   ├─ kernel  v6.1.2         (git@gitlab.company.com:firmware/kernel.git)
   └─ hal     main           (git@gitlab.company.com:firmware/hal.git)

$ haw status
REPO      BRANCH   HEAD      DIRTY  DRIFT
kernel    v6.1.2   a1b2c3d4  -      -
hal       main     9f8e7d6c  yes    -
app-mqtt  release  4d5e6f7a  -      YES

$ haw change start FEAT-42 --repos kernel,app-mqtt
changeset `FEAT-42` started across 2 repo(s):
  kernel    -> change/FEAT-42
  app-mqtt  -> change/FEAT-42
```

Output is colored on a TTY and plain when piped (`NO_COLOR` honored) — one scheme
everywhere: **cyan** names, **yellow** revs and branches, dim SHAs, **green** clean,
**yellow** dirty, **red** drift.

## The manifest

One file declares your **repos** and composes them into **stacks**. A repo is shared,
never copied. The committed lockfile pins each one to an exact SHA.

```toml
[remote.internal]
url = "git@gitlab.company.com:firmware"

[repo.kernel]
remote = "internal"
repo   = "kernel.git"
rev    = "v6.1.2"        # tag or SHA => pinned and reproducible
groups = ["firmware"]

[repo.hal]
remote = "internal"
repo   = "hal.git"
rev    = "main"          # branch => follows HEAD, until you lock it

[repo.app-mqtt]
url    = "git@github.com:acme/app-mqtt.git"
rev    = "release/2.x"
path   = "apps/mqtt"     # optional; defaults to the repo name

[stack.gateway]
repos = ["kernel", "hal", "app-mqtt"]

[stack.sensor-node]
repos = ["kernel", "hal"]         # shares kernel + hal, no duplication

[overlay.dev.repo.kernel]
rev = "main"                      # `haw sync --overlay dev` follows main for kernel
```

On disk, stacks reuse the same clones — and there is never a symlink:

```
mystack/
├── haw.toml            # manifest (intent)
├── haw.lock            # lockfile (resolved SHAs, committed)
├── kernel/             # real, complete git repo
├── hal/                # real, complete git repo
└── app-mqtt/           # real, complete git repo
```

Sharing objects across stacks on one machine is opt-in via git's native `alternates`
(`git clone --reference`, enabled by `haw sync --shared`) — a text file, not a symlink.

## Secrets & tokens

`haw` never stores a credential. It reads forge tokens from the environment at call
time and uses them only for API requests (opening PRs, reading CI). Git transport auth
stays with your existing SSH keys or git credential helper — `haw` doesn't touch it.

| Forge | Read in order (first set wins) |
|-------|--------------------------------|
| GitHub | `HAW_GITHUB_TOKEN` → `GITHUB_TOKEN` → `GH_TOKEN` → `HAW_FORGE_TOKEN` |
| GitLab | `HAW_GITLAB_TOKEN` → `GITLAB_TOKEN` → `HAW_FORGE_TOKEN` |

```bash
export GITHUB_TOKEN=$(gh auth token)     # or any PAT, in your shell / CI secret store
```

Read-only composition (`sync`, `status`, `tree`, `verify`) needs no token at all — only
the forge features do.

## Command surface

```
haw                              Open the TUI cockpit (no subcommand)
├── init <manifest-url|path>     Bootstrap a workspace from a manifest
├── sync [--stack S] [--shared]  Clone/pull repos to the state in haw.lock
├── lock / pin / unpin           Resolve revs -> haw.lock / pin to checkouts / restore
├── switch <stack>               Materialize a different stack in the workspace
├── status                       Aggregated fleet status (dirty/ahead/behind per repo)
├── grep <pattern> [--stack S]   git-grep across every cloned repo at once
├── run '<cmd>'                  Run a command across repos, in parallel
├── tree                         Print the stack -> repo tree
│
├── repo   add|remove|list       Edit repos in the manifest
├── stack  add|remove|list       Edit stacks in the manifest
│
├── verify                       Assert tree == haw.lock; exit 3 on drift (CI gate)
├── build / test                 Run each repo's declared build/test command, in parallel
├── hooks  install|list          Git integrity pre-commit + lifecycle hooks (.haw/hooks)
├── evidence                     Bundle manifest+lock+audit+status for audits
│
├── change                       Cross-repo feature ("changeset") workflow
│   ├── start <id> [--repos ..]  Create one branch across the affected repos
│   ├── status                   Per-repo branch + PR/MR review + CI dashboard
│   ├── request                  Open linked PR/MRs on GitHub/GitLab for each repo
│   ├── goto                     Interactive picker; cd into a repo
│   ├── snapshot save|restore    Save/restore the multi-repo state of a changeset
│   └── land                     Merge PR/MRs in dependency order
│
├── merge                        Parallel collaborative merge
│   ├── plan <source>            Slice a big merge into per-directory conflict units
│   ├── resolve <slice>          Resolve one slice (--take ours|theirs, or by hand)
│   └── status / cleanup / abort Track, seal, or undo the planned merge
│
├── import --from <west.yml|default.xml>   Convert a west/repo manifest to haw.toml
└── dash                         Open the fleet dashboard (same as bare `haw`)
```

Each verb is one guessable word; old names (`graph`, `forall`, `freeze`, `tui`) stay as
hidden aliases. Full lexicon: [docs/CLI-DESIGN.md](docs/CLI-DESIGN.md).

## The TUI cockpit

Keyboard-first and modal, in the spirit of k9s. The loop is **read → drill in → act**:
see a repo's branch, SHA, and status; open a PR's reviewers and checks; watch a CI run's
progress — then merge or approve it, without leaving the terminal. Everything heavy runs
on a background worker, so the UI never freezes.

```text
 haw ▸ ~/work/gateway ───────────────────────── stack: gateway   lock: ✓   repos: 3/3
──────────────────────────────────────────────────────────────────────────────────────
   REPO        BRANCH ▲      HEAD       DIRTY   DRIFT   ↑ / ↓    MERGE
   kernel      v6.1.2        a1b2c3d4     ·       ·      0 / 0     —
 ◉ hal         main          9f8e7d6c    yes      ·      2 / 0     —
▸⚠ app-mqtt    release/2.x   4d5e6f7a     ·      DRIFT   0 / 5     —
──────────────────────────────────────────────────────────────────────────────────────
 hal  ›  path hal/   branch main (ahead 2)   dirty   locked 9f8e7d6c   grp firmware
──────────────────────────────────────────────────────────────────────────────────────
 [s]ync [f]iles [x]shell [!]exec [/]filter [p]roblems [:]cmd [Enter]drill [?]help
```

**See problems first.** Any repo that has drifted from the lock, is dirty, or isn't
cloned turns red with a ⚠. Press `p` to filter the fleet down to just those rows.

**Stay current, hands-free.** The grid auto-refreshes (~5s idle, or `F5`/`ctrl-r` on
demand) without disturbing input or in-flight work. Network views stay on-demand.

**Find and order.** `/` is a fuzzy filter (`/knl` matches `kernel`). `>`/`<` move the
sort column, `.` toggles direction; a `▲`/`▼` caret marks it.

**Search the whole fleet.** `:grep <pattern>` runs `git grep` across every cloned repo
and lists the hits; `Enter` opens the file at the matching line. Same as `haw grep` on
the CLI.

**Drill in (`Enter`).** On a repo: a scrollable git detail — branch, SHA, `status`,
recent `log`, diffstat, remotes. On a PR/MR: reviewers, checks, body, URL. On a CI run:
a live progress bar, its jobs and steps, the runner, and the logs. `j/k` and
`PageUp`/`PageDown` scroll; `b` goes back.

**Browse files and shell in.** `f` opens a file browser for the cursor repo — local
clone by default, or the forge over the API with `R` when the repo isn't checked out;
`Enter` walks into a directory or opens a file. `x` drops you into a shell in the repo;
`!` runs one command there and shows the output.

**Act (confirm-gated — these reach the network).** `M` merges a PR/MR, `A` approves,
`C` checks its branch out locally to review; `F` fetches a single repo. Cross-repo
changesets `R` request and `L` land in dependency order. Each asks `y/n` first.

**Mark and batch.** `space` marks repos (shown `◉`); with marks set, `s` and `r` act on
the marked set instead of the cursor row.

**Views.** stacks → fleet grid → repo detail; changesets with per-repo PR/MR + CI cells;
fleet-wide open PR/MRs (`m`) and recent CI runs (`i`); governance (`v`) for plugins,
SBOM/provenance, and findings. `o` opens the cursor row in your browser.

**Command bar `:`** mirrors the CLI, so learning one teaches the other: `:sync`,
`:switch NAME`, `:grep TODO`, `:run CMD`, `:sh CMD`, `:problems`, `:prs`, `:ci`,
`:theme NAME`, `:help` — and a bare `:name` jumps the cursor to that repo.

**Themes.** Six built-in skins — `catppuccin` (default), `dracula`, `nord`, `gruvbox`,
`solarized`, `monochrome`. `NO_COLOR` forces `monochrome`; `HAW_THEME=<name>` sets one
at startup; `:theme <name>` switches live.

Full keymap: [docs/CLI-DESIGN.md](docs/CLI-DESIGN.md#tui-keymap).

## Demos

Rendered with [VHS](https://github.com/charmbracelet/vhs) from the tapes in
[`demo/`](demo/); CI re-renders them on every CLI/TUI change, so they never drift.

**[Try the cockpit in your browser →](https://nastwinns.github.io/hawser/)** — real
ratatui widgets over [Ratzilla](https://github.com/ratatui/ratzilla), Rust compiled to
WASM, no server. Source: [`site/`](site/).

The TUI demo above runs against a built-in controller (`haw dash --demo`) — no
workspace, git, or network needed — so its PR/MR and CI views are always populated.
Feature-by-feature CLI walkthroughs, paced to read along:

| Tape | Teaches |
|------|---------|
| [`cli-compose`](demo/cli-compose.gif) | `tree` → `sync` → `status` → `lock` → `pin` → `switch` |
| [`cli-changeset`](demo/cli-changeset.gif) | `change start` / `status`; where `request` / `land` open PR/MRs |
| [`cli-run-verify`](demo/cli-run-verify.gif) | parallel `run`, and `verify` as a CI drift gate (exit 3) |
| [`cli-merge`](demo/cli-merge.gif) | the collaborative merge: `plan` → `resolve` → `cleanup` |

## Architecture

`haw-tui` depends only on `ratatui` — it renders and dispatches, and knows nothing about
git or the network. Every side effect crosses a `Controller` trait, which lets the whole
cockpit run headless in tests against a fake. Heavy work runs off the UI thread over
`Job`/`Outcome` channels. The forge layer hides GitHub (octocrab) and GitLab (reqwest)
behind one `Forge` trait.

The full write-up — crate graph, the concurrency model, the forge abstraction, the
reproducibility contract — is in **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)**.

| Crate | Role |
|-------|------|
| [`haw-core`](crates/haw-core) | Manifest, lockfile, resolver, workspace, changesets — the domain logic |
| [`haw-git`](crates/haw-git) | Git backend: gitoxide reads, `git` shell-outs for plumbing |
| [`haw-forge`](crates/haw-forge) | GitHub/GitLab behind one `Forge` trait; changeset + fleet orchestration |
| [`haw-merge`](crates/haw-merge) | Collaborative merge: plan/resolve/cleanup/abort |
| [`haw-tui`](crates/haw-tui) | The ratatui cockpit — renders and dispatches, nothing more |
| [`hawser`](crates/hawser) | The `haw` binary: clap CLI, thin glue |

## Development

```bash
cargo test --workspace                                # unit + integration
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Covered: manifest parse + validation, TOML round-trip, resolver + overlay precedence,
lockfile determinism (byte-identical, LF-only, cross-OS in CI), changeset lifecycle, the
full collaborative merge against real git repos, golden CLI snapshots
(`crates/hawser/tests/golden.rs`), forge orchestration against a fake forge, and the
cockpit logic (filter, sort, marks, command bar, drill-ins, grep, themes).

## Documentation

Published at **[nastwinns.github.io/hawser/docs](https://nastwinns.github.io/hawser/docs/)**
(mdBook, rebuilt on every push). Sources:

| Doc | What |
|-----|------|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crate layout, concurrency model, forge abstraction, data flows |
| [docs/CLI-DESIGN.md](docs/CLI-DESIGN.md) | Full CLI lexicon + TUI keymap |
| [docs/EXTENDING.md](docs/EXTENDING.md) | Extensions, plugins, hooks, auth, CI/CD integration |
| [docs/PLUGINS.md](docs/PLUGINS.md) | Writing subcommand plugins — `haw <name>` runs `haw-<name>` from PATH |
| [docs/COMPLIANCE.md](docs/COMPLIANCE.md) | Tool qualification, SBOM/CRA, crypto/signing, GDPR |
| [docs/INSTALL.md](docs/INSTALL.md) | Full install matrix + signature verification |

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
