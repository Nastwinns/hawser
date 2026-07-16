# Plugin ecosystem — architecture study & roadmap

Status: **design, validated** (2026-07-16). This document records an architecture
review of a proposed 7-plugin ecosystem, the corrected design that survives that
review, and a costed roadmap. Nothing here is implemented yet — it is the plan.

> The original proposal used stale `keel-*` names. The project was renamed; the
> plugin dispatcher resolves `haw-<name>` only (`crates/hawser/src/main.rs`,
> `fn plugin`). All names below are `haw-*`.

## The proposal, in one table

| Phase | Plugin | Claimed role |
|-------|--------|--------------|
| Setup | `haw-env` | Guarantee an identical toolchain / flag gaps |
| Integration | `haw-git-gate` | Clean/secure the input before a commit lands |
| Build | `haw-build` | Unify `npm`/`make`/`cargo` behind one command (a "Dispatcher") |
| Audit | `haw-compliance` | Generate an SBOM |
| CI/CD | `haw-ci` | Make haw native in GitHub/GitLab/Jenkins runners |
| Distribution | `haw-artifact` | Ship binaries with metadata (the `haw.lock` SHA) |
| Quality | `haw-trace` | Link code to business requirements (JIRA/DOORS) |

Proposed mechanisms: a **Dispatcher pattern** (plugins hold no build logic —
they read `haw.toml`, run the declared command, manage input/output); a single
in-process Rust `trait Plugin { pre_run; run; post_run }`; and a TUI-vs-CI split
(library engine + TTY detection → headless).

## Verdict

**As written: no-go.** **Corrected: go.**

The proposal's *philosophy* is already haw's philosophy — and in several places
it is already *shipped code*. The review (architect, product, adversarial, and
Rust-feasibility perspectives) converged on four conclusions:

1. **The in-process `trait Plugin` regresses a shipped guarantee.** haw's plugin
   contract is deliberately **out-of-process** (`haw-<name>` executables, the
   `haw.plugin/1` JSON contract over `HAW_JSON` env + stdin, exit-code
   propagation). That buys isolation ("a broken plugin can't crash haw"),
   fail-open, and polyglot authoring (the shipped example plugin is POSIX shell).
   Compiling third-party plugins into the `haw` binary throws all three away and
   contradicts the `unsafe_code = "forbid"` / memory-safety selling point. **Keep
   the out-of-process wire; do not add an in-process trait to `haw`.**

2. **Two of the seven already exist; two more are hooks, not plugins.**
   - `haw-build` **is** `haw build` / `haw test` — `build_or_test()` already reads
     each repo's `build =` / `test =` command from the manifest and fans them out
     in parallel. The "Dispatcher" is shipped.
   - `haw-ci` **is** the existing headless contract — `sync --locked`, `verify`
     (exit 3 on drift), `--format json` everywhere, stable exit codes, plus the CI
     sketches in [EXTENDING.md](EXTENDING.md). What's new (PR comments, merge
     gating) is a thin *wrapper*, not an engine.
   - `haw-env` collapses into a toolchain field in the `haw evidence` bundle plus
     a `pre-sync` hook.
   - `haw-git-gate` collapses into an installable **hook** (reusing
     `haw hooks install`), not a new subcommand.

3. **Three are genuinely new** — and they are the ones that matter commercially:
   `haw-compliance` (SBOM), `haw-artifact` (signed provenance), `haw-trace`
   (requirements traceability). `haw-trace` is **out of the tool's declared
   scope** ("haw decides which revision enters a build; it does not verify code")
   and a multi-quarter, per-customer integration — it is **deferred to services /
   the qualification kit**, not a product plugin.

4. **The business case is inverted.** The proposal leads with dev-productivity
   plugins and treats compliance as a late "killer feature." But the productivity
   surface is already the free core, and [COMMERCIALIZATION.md](COMMERCIALIZATION.md)
   is explicit: *you sell reproducible, certifiable baselines, not the workflow.*
   The wedge is **an SBOM-complete, signed `haw evidence` bundle** — the first
   artifact a regulated buyer pays for.

## The corrected architecture

Everything rides the seams haw already ships. No core fork, no in-process trait.

### 1. The wire contract stays frozen

`haw-<name>` + `haw.plugin/1` (context on `HAW_JSON` env **and** stdin) +
exit-code propagation is the **sole** community boundary. See [PLUGINS.md](PLUGINS.md).

### 2. Lifecycle phases as a convention over the same wire

haw invokes **explicitly registered** plugins at lifecycle points, passing a phase
argument (`haw-<name> --haw-phase post-build`) and the JSON context extended with
a `"phase"` field, then collects a JSON report from stdout.

Registration is a manifest table — **never** an auto-run of every `haw-*` on
`PATH` (that is a supply-chain footgun: a poisoned `PATH` on a shared runner would
exec an attacker's `haw-build` with the developer's forge tokens).

```toml
[plugins]
# plugin name -> the phases it subscribes to
haw-compliance = ["post-build"]
haw-artifact   = ["post-build", "post-land"]
```

### 3. Lifecycle hook points (a small fixed set — not a rigid 7-stage pipeline)

haw is an orchestrator that shells out, not a build system that owns a pipeline.
Extend the existing hook enum (`PreSync/PostSync/PreLock/PostLock/PostSwitch/
PostChangeStart`) with the missing points. **`haw build`/`test` fire no hooks
today** — that is the one real plumbing gap the new plugins need closed.

```
pre-sync   / post-sync          [exist]
pre-lock   / post-lock          [exist]
post-switch                     [exist]
post-change-start               [exist]
pre-build  / post-build         [ADD — unblocks compliance, artifact]
pre-test   / post-test          [ADD — unblocks quality gates]
pre-request / post-land         [ADD — unblocks artifact-on-merge]
```

The proposal's seven stages map cleanly onto these (setup→pre-sync,
integration→post-sync, build→pre/post-build, audit→post-build, dist→post-land,
quality→post-test), proving the hook set is sufficient without a pipeline
straitjacket. `pre-*` failures may gate; `post-*` failures degrade to warnings
(matching the shipped hook semantics), preserving fail-open.

### 4. An optional `haw-plugin` SDK crate

For authors who *want* the `pre_run / run / post_run` ergonomics, ship a Rust SDK
crate that parses `HAW_JSON`, dispatches on `--haw-phase`, and emits a versioned
report — but **compiles to a standalone `haw-<name>` binary**. The trait lives in
the plugin's address space, never in `haw`. This delivers 100% of the proposal's
developer experience with zero isolation cost. (It also ships the Windows `.bat`
shim story, mirroring the hook engine.)

### 5. TUI-vs-CI is already done

`haw-core` is the library; `haw-tui` depends only on it; `hawser` injects a
controller and detects a TTY (`IsTerminal`). Bare `haw` opens the cockpit;
`haw <cmd> --format json` is headless. `haw-ci` is not a plugin — it is this
existing headless mode, optionally wrapped by reference CI templates.

### Governance guardrails (a plugin ships only if it meets all of these)

- Out-of-process, on `haw.plugin/1`, with **zero** dependency on any `haw` crate.
- Registered explicitly — never auto-run from `PATH`.
- **Never writes core-owned files** (`haw.toml`, `haw.lock`, `.haw/` state).
  Plugins consume via `--format json`; core owns its formats.
- **Orchestrates, never reimplements.** An SBOM plugin drives Syft /
  cargo-cyclonedx; a secret gate drives gitleaks / Semgrep — and surfaces their
  verdict verbatim. A shallow hand-rolled scanner is *compliance theater*: it
  manufactures false assurance, the worst outcome for a safety-critical buyer.
- First-party bundled plugins meet the same 3-OS bar (Linux/macOS/Windows) and
  deterministic, signed release as core.

## Reclassifying the seven

| Proposed | Reality | Lands as |
|----------|---------|----------|
| `haw-build` | Shipped (`haw build`/`test`) | **Nothing to build** (optional: build-system auto-detect when no `build =`) |
| `haw-ci` | Shipped (`--format json`, `verify` exit 3) | **CI recipes** + a thin PR-comment wrapper |
| `haw-env` | Mostly shipped (`sync`/`--shared`, evidence) | Toolchain field in `haw evidence` + a `pre-sync` hook |
| `haw-git-gate` | Mostly shipped (`hooks`/`verify`) | An installable **hook** wrapping gitleaks/Semgrep |
| `haw-compliance` | **New** (the wedge) | First-party bundled plugin → SBOM into `haw evidence` |
| `haw-artifact` | **New** | Signed provenance on release (extends `xtask dist`/evidence) |
| `haw-trace` | **New, out of scope** | **Deferred** to services / qualification kit |

## Roadmap (value-first, corrected)

### Phase A — Make `haw evidence` SBOM-complete (the wedge)
Composition-level SBOM (CycloneDX **and** SPDX 2.3, NTIA-minimum fields) built
from the pinned baseline haw already knows (repos + SHAs in `haw.lock`),
orchestrating per-ecosystem scanners and normalising to deterministic, sorted
output. Folds into the existing evidence bundle.
*Done when:* the bundle emits valid CycloneDX + SPDX that validate against upstream
schemas, byte-deterministic cross-OS, accepted by one ASPICE/DAL-C–D design partner.

### Phase B — Sign the evidence & bind it to releases (`haw-artifact`)
Attach the signed lock + SLSA/in-toto provenance to a release so every binary is
provably tied to a baseline. Extends `xtask dist`.
*Done when:* a released artifact carries a verifiable "what went in, from where, at
which id"; the evidence bundle is signed (cosign/sigstore); a reviewer can prove
the baseline was not altered post-approval.

### Phase C — CI recipes + org policy (not a Jenkins plugin)
Reference GitHub Action + GitLab template running `sync --locked → verify →
evidence`, commenting PRs and blocking on drift via the shipped exit codes. Jenkins
only on paid demand.
*Done when:* an outside team wires the Action in under 15 minutes and gets PR
comments + a merge block on drift with zero bespoke code.

### Phase D — Traceability (services / qualification kit, design-partner-gated)
The requirements→test matrix **for the tool itself** ships in the qualification
kit. A customer-project JIRA/DOORS matrix is professional services co-specified
with the first Compliance design partner — never a speculative v1 plugin.

### Cross-cutting infra (unblocks A–C)
`--haw-phase` dispatch, the extended hook points (wire `haw build`/`test`), the
`[plugins]` registration table, and the optional `haw-plugin` SDK.

## Effort estimate

One experienced Rust dev. Person-days: optimistic – **likely** – pessimistic.

| Item | Status | Days |
|------|--------|------|
| `--haw-phase` lifecycle dispatch + report collection | partial | 3 – **5** – 8 |
| Extend + fire hook points (build/test/request/land) | partial | 2 – **3** – 5 |
| `[plugins]` manifest registration + validation | new | 2 – **3** – 5 |
| `haw-plugin` SDK crate | new | 4 – **6** – 10 |
| `haw-compliance` (SBOM, CycloneDX+SPDX, orchestrated) | new | 8 – **14** – 22 |
| `haw-artifact` (signing + provenance) | new | 5 – **8** – 13 |
| `haw-git-gate` hook (net-new slice) | partial | 2 – **4** – 6 |
| `haw-env` toolchain capture (net-new slice) | partial | 2 – **3** – 5 |
| `haw-build` / `haw-ci` | shipped | 0 (docs) |
| build-system auto-detect (optional) | new | 1 – 2 |
| `haw-trace` | new, deferred | multi-quarter (~40–80+, own initiative) |

**Credible v1 (Phases A–C, excluding trace): ≈ 28 – 43 – 69 person-days**, i.e.
~10–11 weeks for one dev including the `fmt`/`clippy -D warnings`/`test` gates and
Windows hardening. **`haw-compliance` (SBOM) is the schedule driver.**

### Top implementation risks
1. **SBOM composition + scanner orchestration** — multi-format completeness,
   NTIA-minimum correctness, merging heterogeneous scanner output deterministically;
   depending on external scanners on `PATH` makes CI flaky.
2. **Report-schema churn** — freeze the `--haw-phase` report JSON *before* the SDK
   ships, or every bundled plugin reworks.
3. **Signing/provenance trust model** — backend choice (sigstore vs minisign), key
   management, and where the signature attaches are unbounded without a decision.

## Rejected mechanisms (explicit non-goals)

- **In-process `trait Plugin` compiled into `haw`** — regresses isolation,
  fail-open, polyglot, and the memory-safety story.
- **Auto-running every `haw-*` on `PATH`** in a lifecycle — supply-chain / token
  exfiltration footgun. Registration is explicit.
- **A naive `scan → sbom.json`** — compliance theater against the CycloneDX+SPDX /
  NTIA / determinism claims in [COMPLIANCE.md](COMPLIANCE.md).
- **Reimplementing dependency scanning or a Jenkins plugin** — orchestrate
  existing tools; ship CI recipes.

See also: [EXTENDING.md](EXTENDING.md) (mechanisms), [PLUGINS.md](PLUGINS.md) (the
wire contract), [ARCHITECTURE.md](ARCHITECTURE.md) (the thin-core invariant &
decision records), [COMPLIANCE.md](COMPLIANCE.md) (SBOM/qualification claims).
