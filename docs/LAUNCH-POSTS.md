# hawser — copy-ready launch posts

Ready-to-paste drafts for the v0.1.0 launch. This file **complements**
[`LAUNCH.md`](LAUNCH.md) (which holds the timing gate, Reddit norms, per-sub
playbook, and media checklist) — it is the actual copy you paste, one block per
channel, plus a Q&A crib sheet.

**Do not post before the timing gate in [`LAUNCH.md §0`](LAUNCH.md) is met.** In
short: v0.1.0 must be installable (`cargo install hawser` from crates.io **or** a
one-line install from the release archives), the repo must be public, and the TUI
gif must exist and be readable on mobile. See the timing note at the bottom.

Real links to use (all live):

- Repo: <https://github.com/Nastwinns/keelson>
- Docs (mdBook): <https://nastwinns.github.io/keelson/docs/>
- Browser TUI demo (ratatui → WASM, no server): <https://nastwinns.github.io/keelson/>
- CLI gif: <https://github.com/Nastwinns/keelson/blob/main/demo/haw-cli.gif>
- TUI gif: <https://github.com/Nastwinns/keelson/blob/main/demo/haw-tui.gif>
- Guided tapes: `demo/cli-compose.gif`, `demo/cli-changeset.gif`,
  `demo/cli-run-verify.gif`, `demo/cli-merge.gif`

Honesty rules for every post below: no invented benchmarks, no "N× faster than X",
no certification claims. Describe what the tool actually does today — composition,
a committed lockfile, a `verify` drift gate, cross-repo changesets on GitHub +
GitLab, a ratatui TUI — and flag SBOM/signing/qualification as roadmap, not shipped.

---

## 1. Show HN

**Title** (≤ 80 chars, no hype):

```
Show HN: haw – reproducible multi-repo stacks + cross-repo PRs, in Rust
```

**Body:**

```
haw assembles a software product out of many independent Git repos — without
submodules, without detached HEADs, without a Python runtime.

A TOML manifest (haw.toml) declares your repos and "stacks" (named compositions
that share the same clones). A committed lockfile (haw.lock) pins every repo to an
exact SHA, so a teammate or a CI machine reproduces byte-identically. On top of
that, one command branches a feature across every affected repo, opens the linked
PRs/MRs on GitHub *and* GitLab, and tracks review + CI state from a k9s-style TUI.

Why I built it: I kept managing a stack spread across ~10 repos with a pile of bash
and git submodules, and every tool I tried solved only one slice. Google's `repo`
and Zephyr's `west` give you a manifest but no lockfile, need Python, and leave you
in detached HEADs (and their symlink layouts fight Windows). RepoFleet does the
issue→branches→PR flow but not reproducible composition. mergetopus does parallel
single-repo merges but nothing multi-repo. haw is the union: reproducible
composition + cross-repo MR orchestration + an optional parallel collaborative
merge, in one binary with one TUI.

What's honest about the scope: haw orchestrates Git and the forge APIs. It does not
reimplement Git's merge engine, replace a forge, or replace your build/domain
toolchain. Reads go through gitoxide; the mutating plumbing shells out to your
system `git`, so `git` must be on PATH.

Status: v0.1.0, Rust, #![forbid(unsafe_code)], Linux/macOS/Windows. It's young —
the commercial/compliance surface (SBOM, signed lockfile, qualification evidence)
is on the roadmap, not shipped. Dual MIT/Apache-2.0.

Repo: https://github.com/Nastwinns/keelson
Try the TUI in your browser (Rust→WASM, no server): https://nastwinns.github.io/keelson/

Feedback very welcome, especially from anyone living the multi-repo life.
```

Post Tue–Thu, ~8–10am ET. Stay on the thread for the first 2 hours.

---

## 2. r/rust

**Title:**

```
haw: reproducible multi-repo stacks + cross-repo PR/MR orchestration, one binary, no Python
```

**Body:**

```
I got tired of holding a product together across ~10 git repos with bash and
submodules, so I built `haw`.

A TOML manifest declares your repos and *stacks* (named compositions that reuse the
same clones); a committed lockfile pins every repo to an exact SHA so CI and
teammates reproduce the exact tree. On top: start a feature branch across N repos at
once, open the linked PRs/MRs (GitHub + GitLab), and watch the whole fleet in a
k9s-style TUI.

The Rust angle:
- One static binary, no Python runtime anywhere (`repo`/`west` need one).
- #![forbid(unsafe_code)] across the whole workspace.
- ratatui cockpit — modal, keyboard-first: `:` command bar, `/` filter, single keys
  act on the cursor row, async refresh so the UI never blocks.
- Reads go through gitoxide (gix) for fast native introspection; only the mutating
  plumbing shells out to system `git`.
- Errors are thiserror enums in the libs, anyhow only in the binary; clap for the
  CLI. Workspace split into haw-core (all domain logic) + thin front-ends.

It's v0.1.0 and young. Repo + gifs: https://github.com/Nastwinns/keelson
You can even try the cockpit in your browser (ratatui compiled to WASM over
Ratzilla, no server): https://nastwinns.github.io/keelson/

Would love feedback on the crate split and the gitoxide/shell-out boundary.
```

If unsure, drop a shorter version in the weekly "what are you working on" thread
first (see [`LAUNCH.md §4`](LAUNCH.md)).

---

## 3. r/embedded

**Title:**

```
For west/repo refugees: a Rust multi-repo tool with a committed lockfile and no Python
```

**Body:**

```
Splitting firmware across shared BSP/HAL/MCAL repos reused across ECUs is routine,
but `west`/`repo` leave you with: no lockfile, a Python runtime to carry, detached
HEADs, and symlink-y layouts that fight Windows.

`haw` gives you:
- A committed lockfile (haw.lock) pinning every repo to an exact SHA → the tree is
  reproducible run-to-run and cross-OS, which is the property audits actually ask
  for. Reproducible baselines matter for functional-safety and process work
  (ISO 26262, ASPICE), where "prove the tree you built is the tree you qualified"
  is a real question. (haw helps you *reproduce* a baseline; it is not itself a
  qualified tool and makes no certification claim — that's roadmap, see below.)
- `haw verify`: asserts the working tree matches haw.lock and exits 3 on drift, so
  you can wire it into CI as a hard gate.
- Plain, full git clones — no detached HEAD, no symlinks — so it behaves on Windows.
- No network or telemetry needed to compose/verify; air-gap-friendly by design
  (offline composition, no phone-home).
- `haw import --from west.yml` (and repo's default.xml) to convert an existing
  manifest and try it on your tree without rewriting anything by hand.
- A fleet TUI to see branch/dirty/drift/ahead-behind across every repo at once.

Roadmap, not shipped yet: SBOM export (CycloneDX/SPDX), signed lockfile /
provenance, and per-standard qualification evidence. So today it's the free,
reproducible core — no compliance overclaim.

Rust, one binary, Linux/macOS/Windows, dual MIT/Apache-2.0.
Repo: https://github.com/Nastwinns/keelson
Browser demo: https://nastwinns.github.io/keelson/

Tell me what's missing for your toolchain — that's exactly the feedback I want.
```

r/embedded is strict on promo tone. Keep it concrete, zero marketing voice.

---

## 4. r/commandline

**Title:**

```
k9s-style TUI for managing a fleet of git repos [gif]
```

**Body:**

```
`haw` — one screen for N git repos: branch, dirty state, drift-vs-lockfile, and
ahead/behind, all live. Modal and keyboard-first like k9s: `:` opens a command bar
mirroring the CLI (`:sync`, `:run git status`, `:prs`, `:ci`), `/` filters the grid,
single keys act on the cursor row, `o` opens the row in your browser. Async refresh,
so the UI never freezes.

Bare `haw` opens the cockpit; there are fleet-wide "open PR/MRs" and "recent CI
runs" views across every repo. Colors on a TTY, plain when piped, NO_COLOR and
CLICOLOR_FORCE honored like ripgrep/bat/eza.

Rust + ratatui, cross-platform. The gifs in the repo are rendered with VHS from tape
files and CI re-renders them on every CLI/TUI change, so they never drift from
reality:

- CLI: https://github.com/Nastwinns/keelson/blob/main/demo/haw-cli.gif
- TUI: https://github.com/Nastwinns/keelson/blob/main/demo/haw-tui.gif

And you can drive the actual cockpit in your browser (ratatui → WASM, no server):
https://nastwinns.github.io/keelson/

Repo: https://github.com/Nastwinns/keelson
```

Gif first — on r/commandline a static repo link underperforms badly.

---

## 5. Answers to likely questions

Paste-ready replies so you can respond in the first 2 hours.

**vs git submodules?**
Submodules pin SHAs but they're painful: recursive update dance, detached HEADs,
and no notion of "a named composition of repos" or a cross-repo feature branch.
haw keeps plain, autonomous full clones (you `cd` in and it's a normal repo), adds
a human-readable manifest + a committed lockfile, and drives the multi-repo branch/
PR workflow on top. You can still use submodules inside any individual repo.

**vs git subtree?**
Subtree vendors another repo's history *into* yours — one repo, merged histories,
and rewrites on update. haw does the opposite: repos stay separate and autonomous,
composed by reference (manifest + lock), never merged into one tree. No history
rewriting, and each repo keeps its own remotes and permissions.

**Why TOML and not YAML/XML?**
It's the lockfile that matters most, and TOML gives a canonical, diff-friendly,
comment-friendly serialization that's easy to review in a PR. `west` uses YAML and
`repo` uses XML; haw can `import --from` both, so you're not locked out.

**Does it work with GitLab?**
Yes. Forges sit behind one `Forge` trait; GitHub and GitLab are both implemented, so
cross-repo changesets open and track PRs *and* MRs. A stack can even mix GitHub and
GitLab repos.

**Is it production-ready?**
It's v0.1.0 — the composition core (manifest, lock, sync, verify, changesets, TUI)
is real and tested, but it's young and the API/formats may still move before 1.0.
Try it on a non-critical workspace first. The compliance surface (SBOM, signing,
qualification evidence) is explicitly roadmap, not shipped.

**Does it need Python / a runtime?**
No. Single Rust binary. It does shell out to your system `git` for mutating
operations (reads use gitoxide), so `git` must be on PATH — that's the only runtime
dependency.

**License?**
Dual MIT OR Apache-2.0, your choice. The core is genuinely free and open; any future
commercial/compliance tier is kept in a separate boundary (see
[docs/COMMERCIALIZATION.md](COMMERCIALIZATION.md)).

**Windows?**
Yes — plain full clones, no symlinks, `PathBuf` everywhere. Windows support is a
first-class goal, precisely where `repo`/`west` struggle.

---

## 6. Timing note

- **Launch only AFTER v0.1.0 is installable** — `cargo install hawser` succeeds from
  crates.io, or the release archives + a one-line install are documented and work on
  a clean machine. A repo that doesn't run burns the one first impression a community
  gives you. Follow [`RELEASE-CHECKLIST.md`](RELEASE-CHECKLIST.md) end-to-end first.
- **Post HN + Reddit the same day** you go live (Tue–Thu, ~8–10am ET is the best
  window for HN and r/rust; r/commandline is fine on weekends since it's gif-driven).
  Space additional subs 2–3 days apart per [`LAUNCH.md`](LAUNCH.md) to avoid spam
  flags.
- **Engage the first 2 hours.** Early comment velocity drives ranking on both HN and
  Reddit. Have the Q&A crib above open, and convert real feedback into GitHub issues
  live.
