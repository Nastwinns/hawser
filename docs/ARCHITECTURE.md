# hawser Architecture

`hawser` is a multi-repo workspace manager. It pins a fleet of git repositories
to exact commits, drives their PR/MR lifecycle across GitHub and GitLab, and
presents the whole fleet through a keyboard-first ratatui cockpit (the `haw`
binary). This document describes how the code is actually organized — the crate
boundaries, the concurrency model, and the invariants that keep it testable.

The design goal that shapes everything below: **domain logic never does I/O it
can't fake.** Git side effects cross the `GitBackend` trait, forge calls cross
the `Forge` trait, and the TUI's side effects cross the `Controller` trait. Each
seam has a production impl and a test fake, so the bulk of the code is unit-tested
without a network or a real git.

## 1. Overview

The workspace (`Cargo.toml`, `resolver = "3"`, edition 2024, `unsafe_code = "forbid"`
workspace-wide) is a set of small crates. `haw-core` holds the domain model and
depends on nothing but serialization crates. Everything else fans out from it.
The binary `hawser` (shipping as `haw`) is the only crate that wires the pieces
together and the only place `anyhow` is allowed.

```
                    +----------------------+
                    | haw-tui              |   depends ONLY on
                    | (ratatui cockpit)    |   ratatui + nucleo + haw-core types
                    +----------+-----------+   — no git, no network.
                               | Controller trait (side-effect seam)
                               v
+---------+   +----------+   +----------+   +-----------+   +-----------+
| hawser  |-->| haw-core |   | haw-git  |   | haw-forge |   | haw-merge |
| (bin)   |   | (domain) |<--| ShellGit |   | Forge     |   | slice/seal|
+----+----+   +----------+   +----------+   +-----------+   +-----------+
     |            ^   ^            (impl of        (octocrab /
     |            |   |         GitBackend)         reqwest)
     +---> haw-git, haw-forge, haw-merge, haw-tui  (binary wires them all)

haw-plugin ....... SDK for out-of-process `haw-<name>` plugin binaries
haw-artifact ..... plugin: SLSA/in-toto provenance + signing
haw-compliance ... plugin: CycloneDX 1.5 + SPDX 2.3 SBOM
haw-git-gate ..... plugin: secret/hygiene gate (gitleaks or heuristic)
xtask ............ release/packaging automation (`cargo xtask dist`)
```

`haw-core` has zero dependency on `haw-git`, `haw-forge`, or `haw-tui`: it
declares the `GitBackend` trait (`haw-core/src/git/mod.rs`) and lets callers
inject an implementation. `haw-git` depends on `haw-core` and implements that
trait; the arrow points from the impl to the trait.

## 2. Crate responsibilities

| Crate | Responsibility | Key public types |
|-------|----------------|------------------|
| `haw-core` | Domain model, no I/O opinions. Manifest → lock → workspace state, resolution, snapshots, audit log, plugin dispatch. | `manifest::Manifest`, `lock::Lockfile` / `LockedRepo`, `workspace::Workspace` / `RepoStatus` / `SyncPlan` / `RepoTask`, `resolver::ResolvedRepo`, `git::GitBackend` (trait) |
| `haw-git` | Production `GitBackend`: shells out to the user's `git`. Bounded fan-out helper. | `ShellGit`, `parallel::fan_out` |
| `haw-forge` | PR/MR + CI orchestration behind the `Forge` trait; forge-agnostic changeset lifecycle. | `Forge` (trait), `github::GitHub`, `gitlab::GitLab`, `ForgeFactory` / `Tokens`, `ForgeError`, `OpenPr` / `CiRun` / `PrStatus` |
| `haw-merge` | Optional mergetopus-style collaborative merge: slice a conflict-heavy merge by top-level path, resolve piecewise, seal into one commit on an integration branch. | `MergeBackend` (trait), `git::GitMerge` |
| `haw-tui` | The `haw` cockpit. Renders and dispatches; knows nothing about git or the network. | `Controller` (trait), `run`, `Snapshot`, `FleetPr` / `FleetCiRun`, `Exit` |
| `haw-plugin` | SDK for authoring out-of-process `haw-<name>` plugin binaries (`haw.plugin/1` context in/`haw.plugin.report/1` out). | `run`, `Report` |
| `hawser` | The `haw` binary. Clap CLI, wires core+git+forge+merge+tui, owns `anyhow` and the actionable error surface. | `CliController`, `DemoController`, `main` |
| `xtask` | Release/packaging: build a release binary, archive under `dist/`, print SHA-256 for the Homebrew formula / Scoop manifest. | `main` |

## 3. The Controller boundary

`haw-tui`'s dependencies are `ratatui`, `nucleo-matcher`, and `haw-core` (for the
plain data types it renders). It has **no** dependency on `haw-git`, `haw-forge`,
or `tokio`. Every side effect the cockpit needs — refreshing status, fetching
PRs, merging, checking out a branch, reading a file tree — is a method on
`Controller` (`haw-tui/src/lib.rs:351`):

```rust
pub trait Controller: Send {
    fn snapshot(&mut self) -> io::Result<Snapshot>;
    fn fleet_prs(&mut self) -> io::Result<Vec<FleetPr>>;
    fn pr_merge(&mut self, repo: &str, number: u64) -> io::Result<String>;
    fn pr_checkout(&mut self, repo: &str, number: u64) -> io::Result<String>;
    fn repo_tree(&mut self, repo: &str, subpath: &str, remote: bool) -> io::Result<Vec<FileEntry>>;
    fn file_content(&mut self, repo: &str, path: &str, remote: bool) -> io::Result<String>;
    // ~25 verbs total; all return io::Result and all are Send.
}
```

This is a deliberate dependency inversion. The TUI is the top of the graph and
depends only on an abstraction; the binary supplies the concrete implementation:

- **`CliController`** (`hawser/src/main.rs:2295`) is the production impl. Each
  method opens a `Workspace`, builds a `ShellGit` backend and/or a `Forge`
  client, and runs the same code paths the CLI subcommands use (e.g.
  `sync_filtered` reuses `Workspace::plan_sync` + `fan_out`).
- **`DemoController`** (`hawser/src/main.rs:3052`) returns canned in-memory data
  and reaches no workspace, git, or network. `haw dash --demo` renders every
  view deterministically for GIF recordings and, crucially, for tests.

Because the seam is a trait object (`Box<dyn Controller>` passed to
`haw_tui::run`), the 59 unit tests in `haw-tui` construct `App` fixtures and
assert on rendered spans and on the `Job`s dispatched to the worker channel —
no terminal, no git, no sockets. The cockpit's logic is exercised headlessly.

## 4. Concurrency model

The cockpit is single-threaded for rendering and drives all blocking work on one
dedicated worker thread. There is no async in the UI at all.

**Why not full async.** The forge clients and git shell-outs are inherently
blocking; making the whole UI async would buy nothing but a runtime and colored
functions. Instead the render loop stays synchronous and offloads blocking work
to a thread, communicating over two `std::sync::mpsc` channels.

**The worker.** `haw_tui::run` (`haw-tui/src/lib.rs:886`) creates a `Job` channel
and an `Outcome` channel, then `spawn_worker` (`:914`) moves the `Box<dyn
Controller>` onto a `std::thread::spawn`. The worker is a serial loop:
`while let Ok(job) = jobs.recv()`, matching each `Job` to a `Controller` call and
sending back an `Outcome`. Serialization is a feature: entering a view while a
job is in flight still enqueues the read; the worker runs it after the current
one (comments at `:1005`, `:1144`), so navigation is never refused.

**The channel enums** (`:426`, `:463`):

```rust
enum Job {
    Refresh, ChangesetPrs(String), FleetPrs, FleetCi, Governance,
    RepoDetail(String), PrDetail(String, u64), CiDetail(String, u64),
    PrDiff(String, u64), CiLogs(String, u64),
    RepoTree(String, String, bool), FileContent(String, String, bool, String),
    Action(&'static str, ActionKind),   // side-effecting verbs (sync, land, merge, ...)
}

enum Outcome {
    Snapshot(Box<io::Result<Snapshot>>), FleetPrs(Box<io::Result<Vec<FleetPr>>>),
    Detail(String, Box<io::Result<String>>),   // shared drill-in (repo git / PR / CI)
    Tree(Box<io::Result<Vec<FileEntry>>>),
    Action(&'static str, io::Result<String>),
    // ...
}
```

Results are boxed to keep the enum small despite carrying large payloads
(snapshots, diffs). The `&'static str` label on `Action` flows through unchanged
so the outcome handler knows what completed.

**Staying responsive.** The event loop (`event_loop`, `:1345`) never blocks on
the worker. Each iteration: drain `outcomes.try_recv()` (non-blocking) and apply
results, opportunistically auto-refresh when idle (5s cadence, suppressed during
input/overlays/in-flight work, `:1525`), draw one frame, then
`event::poll(Duration::from_millis(120))` for input. `app.busy: Option<&'static
str>` gates a spinner and prevents double-dispatch; it is cleared when the
matching `Outcome` arrives. Network views (Prs/Ci/Governance) are strictly
on-demand — the idle auto-refresh only touches the local status snapshot.

Separately, cross-repo CLI work (sync, `run`) uses `haw_git::parallel::fan_out`
(`haw-git/src/parallel.rs`): bounded fan-out across repos with plain
`std::thread::scope` and a shared atomic index, `jobs.clamp(1, items.len())`
workers. No tokio there either.

### The octocrab `runtime.enter()` gotcha

`Forge` is a synchronous trait, but `octocrab` is async. `github::GitHub` owns a
private `tokio::runtime::Builder::new_current_thread().enable_all().build()`
runtime (`haw-forge/src/github.rs:16`) and calls `runtime.block_on(...)` for each
request — a synchronous facade over an async client, one worker thread, no shared
global runtime.

The subtle bug this guards against: building the octocrab client is not itself an
`await`, but internally it spawns a `tower::buffer` worker task, and that spawn
panics with **"no reactor running"** if there is no live Tokio reactor in the
current thread's context. `block_on` establishes that context only for the future
it drives — not for the synchronous `builder.build()` call. The fix is the guard
in `client` (`github.rs:32`):

```rust
// octocrab's client spawns a tower::buffer worker on build, which needs a
// live Tokio reactor; enter the runtime so the spawn doesn't panic.
let _guard = self.runtime.enter();
builder.build()...
```

`runtime.enter()` returns an `EnterGuard` that installs the reactor for the
current scope, so the `tower::buffer` spawn finds a reactor. A dedicated
regression test builds the client with an empty token and no network to keep this
from silently breaking again (`github.rs:626`,
`client_builds_inside_runtime_without_panic`).

## 5. The Forge abstraction

`Forge` (`haw-forge/src/lib.rs:130`) is one trait with two production impls:

- **`github::GitHub`** — `octocrab` (REST v3) over the private current-thread
  runtime described above. Supports github.com and Enterprise (`/api/v3` base).
- **`gitlab::GitLab`** — `reqwest::blocking::Client` against REST v4. No runtime
  needed; MRs map onto the forge-neutral PR vocabulary.

`ForgeFactory::client_for` (impl `Tokens`, `:273`) picks the impl from the
manifest's explicit `forge =` key if present, else by URL host substring
(`detect`, `:347`), reads tokens from the conventional env vars (falling back to
a logged-in `gh auth token`, `:261`), and returns a `Box<dyn Forge>`.

**Cheap-list vs detail-drill.** The trait splits deliberately into cheap fleet
scans and expensive drill-ins so the fleet views load fast and detail is fetched
only on `Enter`:

- `list_open_prs` / `list_ci_runs` — one bounded call per repo, capped at
  `OPEN_PRS_LIMIT = 25` / `CI_RUNS_LIMIT = 15` to keep request counts bounded on
  busy repos. Returns forge-neutral `OpenPr` / `CiRun` rows.
- `pr_detail` / `ci_run_detail` / `pr_diff` / `ci_logs` / `file_blob` — the
  drill-in fetches, each returning plain text capped by `DIFF_LINE_CAP = 600`,
  `LOG_LINE_CAP = 800`, `FILE_LINE_CAP = 600` via `cap_lines` (which appends a
  "truncated, N more line(s)" note).

**Media-type handling.** octocrab decodes JSON, but diffs, raw blobs, and logs
are plain text. `GitHub::get_text` (`github.rs:77`) sidesteps octocrab with a
small blocking `reqwest` GET carrying a custom `Accept` header and following
redirects, returning `Ok(None)` on 404:

- unified diffs: `Accept: application/vnd.github.v3.diff` (the pulls endpoint
  returns the diff verbatim);
- raw file contents: `application/vnd.github.raw`;
- Actions job logs: served via a 302 redirect to a signed URL (expired logs
  surface as a clear message, not an error).

The plain-text-report contract (no ANSI; the caller styles it) is what lets the
same detail strings render identically in the CLI and in the TUI's scrollable
detail view.

## 6. Reproducibility model

The core contract is a three-stage pipeline: **manifest → lock → state.**

1. **`haw.toml` (manifest)** — `manifest::Manifest` (`model.rs:16`): remotes,
   repos, stacks, overlays. Human-authored intent. A repo's `rev` is a
   branch/tag/sha *reference*.
2. **`haw.lock` (lockfile)** — `lock::Lockfile` (`lock/mod.rs:34`),
   `LOCK_VERSION = 1`, `#[serde(deny_unknown_fields)]`. Machine-generated. Each
   `LockedRepo` pins `rev` (the exact resolved SHA), `source_rev` (the manifest
   ref it was resolved from), and `branch` (repos are never left detached). The
   lock covers **all** repos in the manifest, not just one stack — so switching
   stacks never rewrites the lock; overlays only take effect on regeneration.
3. **Workspace state** — the `.haw/` directory: current stack, snapshots, audit
   log. `Workspace` (`workspace/mod.rs:54`) reads the manifest + lock and plans
   sync (`plan_sync` → `SyncPlan` of `RepoTask`s targeting each locked SHA).

**The lock is the proof/audit artifact.** It is deterministic and LF-only — a
golden test (`hawser/tests/golden.rs::lockfile_is_deterministic_and_lf_only`)
asserts identical bytes across two runs on the same inputs, no CRLF, trailing
newline. That determinism is what makes the lock committable and diffable as a
build-provenance record; `haw-compliance` and `haw-artifact` consume the pinned
`rev`s directly to emit SBOMs and SLSA provenance.

**Drift.** `RepoStatus` (`workspace/mod.rs:99`) carries `head` (the repo's actual
`HEAD`), `locked_rev` (what `haw.lock` says), and `drift: bool` — true when HEAD
differs from the locked rev. `Workspace::status` computes it per repo;
`haw status`/`haw verify` and the cockpit's fleet grid surface it. `pin`
(`:257`) does the inverse: rewrite the lock from current HEADs (no network),
turning the working state into the new pinned truth.

## 7. Error handling

Two-layer strategy, split cleanly at the binary boundary:

- **Libraries use typed errors** via `thiserror`: `ForgeError`
  (`haw-forge/src/lib.rs:116` — `MissingToken`, `UnknownForge`, `Api`, ...),
  `LockError`, `WorkspaceError` / `SyncError`, `GitError`, `ManifestError`,
  `MergeError`. Callers can match on the variant. The `Controller` trait narrows
  these to `io::Result` at the TUI seam (`io::Error::other`), because the cockpit
  only ever renders the message.
- **The binary uses `anyhow`.** `hawser` is the only crate that depends on
  `anyhow`; `run()` returns `anyhow::Result` and adds `.context(...)` at call
  sites. `main` (`main.rs:609`) prints the top-level `error:`, walks
  `err.chain().skip(1)` for causes, and — the actionable part — runs `hint_for`
  (`:632`) over the lowercased error text to attach a one-line fix: no
  manifest → `haw init`; missing token → set `HAW_GITHUB_TOKEN` / `gh auth
  login`; "drift"/"lock" → `haw sync`; "not a git repo" → `haw sync` to clone.

Workspace lints (`Cargo.toml`) set `clippy::unwrap_used` and
`clippy::expect_used` to `warn` across the workspace; test modules opt back in
with `#![cfg_attr(test, allow(...))]`.

## 8. Testing

~173 tests, layered to match the seams:

- **Pure/unit** in each crate: `haw-core` (manifest edit, lock round-trips,
  change lifecycle, resolver, snapshots), `haw-forge` (`repo_coords` for every
  URL shape, `detect`, `cap_lines`, `progress_bar`).
- **`haw-tui` (59 tests)** drive `App` state fixtures (e.g. `fleet_app()`) and
  assert two things: rendered `Span` contents/colors from the pure `draw_*`
  helpers, and the exact `Job` dispatched onto the worker channel after a
  keypress (`rx.try_recv()` → `Ok(Job::FleetPrs)` etc.). No terminal, no
  network — the `Controller` seam and the channel make the whole cockpit
  headlessly testable.
- **`FakeForge` / `FakeGit`** (`haw-forge/tests/orchestrate.rs`): the changeset
  orchestration (request/status/land) runs against in-memory fakes injected via
  `FakeFactory`, so cross-repo lifecycle logic is verified with no HTTP.
- **Golden end-to-end** (`hawser/tests/golden.rs`): builds real git repos in
  tempdirs, runs the actual `haw` binary, and asserts normalized stdout against
  golden strings — `tree`, `status` + the dirty-repo exit-code-3 CI contract,
  `sync`, the stable `haw.status/1` JSON schema, and lockfile determinism. These
  run on the CI matrix, so passing means the shipped binary behaves.

The `Forge`/`GitBackend`/`Controller` triad is the reason this coverage is
cheap: every expensive dependency has a fake, and the one place they're wired to
real I/O — `hawser` — is covered by the golden binary tests.
