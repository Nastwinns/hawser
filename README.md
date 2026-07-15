<!-- markdownlint-disable MD033 MD041 -->
<div align="center">

```
██╗  ██╗███████╗███████╗██╗     ███████╗ ██████╗ ███╗   ██╗
██║ ██╔╝██╔════╝██╔════╝██║     ██╔════╝██╔═══██╗████╗  ██║
█████╔╝ █████╗  █████╗  ██║     ███████╗██║   ██║██╔██╗ ██║
██╔═██╗ ██╔══╝  ██╔══╝  ██║     ╚════██║██║   ██║██║╚██╗██║
██║  ██╗███████╗███████╗███████╗███████║╚██████╔╝██║ ╚████║
╚═╝  ╚═╝╚══════╝╚══════╝╚══════╝╚══════╝ ╚═════╝ ╚═╝  ╚═══╝
      ⚓  the beam that binds the bricks  ⚓
```

**Reproducible multi-repo product composition + cross-repo MR orchestration. In Rust.**

[![build](https://img.shields.io/badge/CI-Linux%20%7C%20macOS%20%7C%20Windows-brightgreen?logo=github)](.github/workflows/ci.yml)
[![crates.io](https://img.shields.io/badge/crates.io-keel--cli-orange?logo=rust)](https://crates.io)
[![rust](https://img.shields.io/badge/rust-1.90%2B-orange?logo=rust)](https://www.rust-lang.org)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![unsafe](https://img.shields.io/badge/unsafe-forbidden-success.svg)](#)
[![platform](https://img.shields.io/badge/platform-linux%20%7C%20macos%20%7C%20windows-blueviolet)](#)

</div>

---

`keel` is a command-line tool (with a TUI) for assembling a software product out of
many independent Git repositories — without submodules, without detached HEADs, and
without a Python runtime. A single declarative manifest describes your **products** and
the **bricks** (repositories) they are composed of; a committed **lockfile** pins every
brick to an exact revision so any teammate or CI machine reproduces the exact same tree.

On top of composition, Keelson orchestrates the day-to-day multi-repo workflow: create a
feature branch across all affected bricks at once, open the matching Pull/Merge Requests
on GitHub and GitLab, and track their review + CI state from one screen.

Keelson runs natively on **Linux, macOS and Windows**. It uses [`gitoxide`](https://github.com/GitoxideLabs/gitoxide)
for fast native introspection and shells out to `git` only for the heavy plumbing.

---

## Quick start

```bash
# install (any of)
cargo install keel-cli          # from crates.io
brew install keelson            # macOS / Linuxbrew
scoop install keelson           # Windows

# bootstrap a workspace from a manifest, then materialize a product
keel init keel.toml
keel sync                       # clones every brick, writes keel.lock
```

A typical session — compose, inspect, branch across repos:

```console
$ keel graph
keel.toml
├─ gateway
│  ├─ kernel    v6.1.2       (git@gitlab.company.com:firmware/kernel.git)
│  ├─ hal       main         (git@gitlab.company.com:firmware/hal.git)
│  └─ app-mqtt  release/2.x  (git@github.com:acme/app-mqtt.git)
└─ sensor-node
   ├─ kernel  v6.1.2         (git@gitlab.company.com:firmware/kernel.git)
   └─ hal     main           (git@gitlab.company.com:firmware/hal.git)

$ keel status
BRICK     BRANCH   HEAD      DIRTY  DRIFT
kernel    v6.1.2   a1b2c3d4  -      -
hal       main     9f8e7d6c  yes    -
app-mqtt  release  4d5e6f7a  -      YES

$ keel change start FEAT-42 --bricks kernel,app-mqtt
changeset `FEAT-42` started across 2 brick(s):
  kernel    -> change/FEAT-42
  app-mqtt  -> change/FEAT-42
```

> Output is colorized on a terminal, plain when piped. Honors `NO_COLOR`.

## How it composes

One manifest declares **bricks** (repos) and **products** (named sets of bricks). A brick is
shared, never duplicated. A committed lockfile pins every brick to an exact SHA.

```
              keel.toml  (intent)                 keel.lock  (pinned SHAs, committed)
                   │                                        │
      ┌────────────┼────────────┐                          ▼
      ▼            ▼            ▼                   reproducible on any machine / CI
 ┌─────────┐  ┌─────────┐  ┌──────────┐
 │ kernel  │  │  hal    │  │ app-mqtt │   ← bricks (full autonomous git clones)
 └────┬────┘  └────┬────┘  └────┬─────┘
      │            │            │
      ├──────┬─────┤            │          products reuse the SAME bricks,
      ▼      │     ▼            ▼          no submodules, no detached HEAD, no symlinks
 ┌──────────┴──┐  ┌────────────┴─────┐
 │  gateway    │  │   sensor-node    │   ← products (compositions)
 │ kernel+hal  │  │   kernel + hal   │
 │  +app-mqtt  │  │                  │
 └─────────────┘  └──────────────────┘
```

---

## Why Keelson exists

Splitting a product into many repositories is common in embedded/automotive/avionics
(shared BSW, HAL, MCAL bricks reused across ECUs) and in microservice backends. The
existing tooling each solves one slice of the problem:

- **Google `repo` / `west`** give you a manifest, but no lockfile, a Python runtime,
  detached HEADs, and (for `repo`) symlink-based layouts that fight Windows.
- **RepoFleet** (Go) nails the *issue → branches across repos → PR/MR* workflow, but has
  no notion of product composition or a reproducible pinned manifest.
- **mergetopus** (Rust) brilliantly parallelizes one big risky merge, but is single-repo.

Keelson is the union that nobody ships: **reproducible product composition** (the `repo`
job, done properly with a lockfile) **+ cross-repo MR orchestration** (the RepoFleet job,
in Rust) **+ optional parallel collaborative merge** (the mergetopus idea), behind one
binary and one TUI.

### What Keelson is not

Keelson orchestrates Git and the GitHub/GitLab APIs. It does **not** reimplement Git's
merge engine, replace a forge, or replace domain toolchains (AUTOSAR generators, DO-178C
traceability tools). It is the orchestration layer those toolchains sit on top of.

---

## Core concepts

**Brick** — one Git repository, cloned as a full autonomous repo (its own `.git`, its own
branches, no detached HEAD). A brick can be shared by several products.

**Product** — a named composition: a set of bricks at chosen revisions. Checking out a
product materializes the union of its bricks at the paths the manifest declares.

**Manifest** (`keel.toml`) — human-authored intent: remotes, bricks, products, overlays.
TOML, for the same reasons Cargo uses it: no indentation traps, no YAML type coercion
("Norway problem"), stable serde ecosystem, clean diffs in review.

**Lockfile** (`keel.lock`) — machine-generated, committed: every brick resolved to an
exact SHA. This is the reproducibility + audit guarantee (a real argument in
automotive/avionics) that `repo` and `west` lack.

**Overlay** — a named set of per-brick overrides (rev, path) applied on top of the
manifest, so variants (dev, bleeding-edge, customer builds) never duplicate brick lists.

**Changeset** — a feature spanning several bricks: one logical branch created across N
bricks, with N linked PR/MRs and an aggregated status.

---

## Layout on disk (no symlinks, ever)

```
myproduct/
├── keel.toml           # manifest (intent)
├── keel.lock           # lockfile (resolved SHAs, committed)
├── kernel/             # real, complete git repo
├── hal/                # real, complete git repo
└── app-mqtt/           # real, complete git repo
```

Bricks are plain clones at their final path — exactly what west does, and the reason it
works on Windows where `repo` struggled. Object sharing across products on one machine is
available as an **opt-in optimization** via git's native `alternates`
(`git clone --reference`), which writes a text file, not a symlink. Keelson uses three
text-based indirections git already provides (`alternates`, the `.git: gitdir:` file, and
its own lockfile) to replace everything `repo` did with symlinks.

---

## Manifest example

```toml
[remote.internal]
url = "git@gitlab.company.com:firmware"

[remote.github]
url = "git@github.com:acme"

# --- bricks ---------------------------------------------------------------

[brick.kernel]
remote = "internal"
repo   = "kernel.git"
rev    = "v6.1.2"        # tag or sha => pinned & reproducible
groups = ["firmware"]

[brick.hal]
remote = "internal"
repo   = "hal.git"
rev    = "main"          # branch => follows head, until locked
groups = ["firmware"]

[brick.app-mqtt]
remote = "github"
repo   = "app-mqtt.git"
rev    = "release/2.x"
path   = "apps/mqtt"     # optional; defaults to the brick name

# --- products -------------------------------------------------------------

[product.gateway]
bricks = ["kernel", "hal", "app-mqtt"]

[product.sensor-node]
bricks = ["kernel", "hal"]        # shares kernel + hal, no duplication

# --- overlays -------------------------------------------------------------

[overlay.dev.brick.kernel]
rev = "main"                      # `keel sync --overlay dev`: kernel follows main
```

---

## Command surface

```
keel
├── init <manifest-url|path>     Bootstrap a workspace from a manifest
├── sync [--product P]           Clone/pull bricks to the state in keel.lock
│                                (resolves + writes lock if absent)  [--shared]
├── lock                         Resolve every brick's rev to a SHA -> keel.lock
├── freeze / unfreeze            Pin all revs to current SHAs / restore to manifest
├── switch <product>             Materialize a different product in the workspace
├── status                       Aggregated fleet status (dirty/ahead/behind per brick)
├── forall -c '<cmd>'            Run a command across bricks, in parallel
├── graph                        Print the product -> brick tree
│
├── brick   add|remove|list      Edit bricks in the manifest
├── product add|remove|list      Edit products in the manifest
│
├── change                       Cross-repo feature ("changeset") workflow
│   ├── start <id> [--bricks ..] Create one branch across the affected bricks
│   │                            [--skip-branch] adopt each brick's current branch instead
│   ├── status                   Per-brick branch + PR/MR review + CI dashboard
│   ├── request                  Open linked PR/MR on GitHub/GitLab for each brick
│   ├── goto                     Interactive picker; cd into a brick
│   ├── snapshot save|restore    Save/restore the multi-brick state of a changeset
│   └── land                     Merge PR/MRs in dependency order
│
├── merge                        (optional) parallel collaborative merge (mergetopus-style)
│   ├── plan <source>            Split a big merge into integration + slice branches
│   ├── resolve <slice>          Resolve one slice with your merge tool
│   └── cleanup                  Promote and remove temporary branches
│
├── import --from <west.yml|default.xml>   Convert a west/repo manifest to keel.toml
└── tui                          Launch the fleet dashboard (ratatui)
```

Key differentiators vs the field: `lock`/`freeze` (reproducibility), `switch <product>`
(composition), parallel `forall` and `sync`, `change request` on **both** GitHub and
GitLab from Rust, and a real fleet **TUI**.

---

## The TUI

A `ratatui` dashboard, because multi-repo state is intrinsically 2-D (N bricks × their
state) and works over SSH — the right shape for embedded/CI users:

- left: product → brick tree
- right: per-brick detail (branch, SHA, dirty, ahead/behind, drift vs lock)
- changeset view: the N branches of a feature, each with PR/MR review + CI status
- keyboard actions: sync, status, switch, start/land a changeset

A richer GUI is possible later via **Tauri**, reusing the exact same Rust core. TUI ships
first: one binary, low cost, on-target.

---

## Status

Phase 1 (double-layer MVP): manifest model, `sync`/`lock`/`status`/`switch`, cross-repo
changesets, read-only TUI, CI matrix. See the docs below for the plan and the roadmap.

## Documentation

| Doc | What |
|-----|------|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crate layout, data flows, phased implementation plan |
| [docs/COMPLIANCE.md](docs/COMPLIANCE.md) | Tool qualification, SBOM/CRA, crypto/signing, GDPR, secure SDLC |
| [docs/COMMERCIALIZATION.md](docs/COMMERCIALIZATION.md) | Editions, licensing, LTS, qualification kit, pricing, GTM |
| [AGENTS.md](AGENTS.md) | Token-saving output rules for AI coding agents in this repo |

## License

Dual-licensed under MIT or Apache-2.0, at your option.
