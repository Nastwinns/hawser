# Production-fit validation — automotive / aviation / defense embedded

Does `haw` answer the real production needs of safety- and security-critical
embedded software (automotive ISO 26262 / ASPICE, aviation DO-178C / DO-330,
defense / air-gapped)? This document maps documented industry requirements to
haw's shipped capabilities, marks the gaps, and ties each gap to the
[plugin-ecosystem roadmap](PLUGIN-ECOSYSTEM.md).

Context: the target profile is a fabless / embedded SoC shop (e.g. Scalinx —
mixed-signal RF SoCs for automotive, aerospace & defense, RADAR), with software
split across many repositories, a **mixed GitHub + GitLab** forge landscape, and
a 10–20 year support horizon.

## Verdict

**haw's core already answers the single most-cited need of all three sectors:
reproducible, auditable, multi-repo baselines.** The gaps are exactly the three
new plugins already on the roadmap (SBOM, signed provenance, traceability) plus
two small additions (named baselines, toolchain capture). Nothing in the industry
requirements contradicts the architecture — they confirm the thesis that the
*reproducible baseline* is the sellable core.

## What the industries actually require (sourced)

- **Reproducible builds for 10–20 years.** ISO 26262-8 CM objective: work products
  "can be uniquely identified and **reproduced** in a controlled manner at any
  time"; UN R156 (SUMS) requires rebuilding legacy ECU software to patch it.
  *"Being able to do things in a reproducible way is all that matters."*
- **Baselines.** ASPICE SUP.8 (BP6) + ISO 26262-8: a baseline is *a precisely
  defined set of uniquely-identified configuration items*, immutable after
  creation, established per milestone with an authorizing role.
- **Pinned multi-repo manifests + validated sync.** The embedded state of the art
  is exactly the `repo`/west pattern: a version-controlled manifest of pinned git
  hashes, `repo sync`, and **a pre-build check that aborts on any hash mismatch**
  vs the pinned manifest.
- **Branch protection / no force-push / PR + review + CI on `main`.** ASPICE CL2
  evidence and the SLSA source track: protected refs, immutable tags, enforced
  change history.
- **Bidirectional traceability** requirements ↔ code ↔ tests. DO-178C (mandatory)
  and ASPICE SWE.6 — typically DOORS / Polarion / LDRA.
- **SBOM.** EO 14028 / NIST SSDF / CRA / defense: CycloneDX **or** SPDX, the seven
  NTIA-minimum fields, a **new SBOM per build**, all transitive components,
  attached as a **signed attestation**.
- **Build provenance & signing.** SLSA (defense baseline = L3: unforgeable,
  isolated), in-toto attestations linking artifact → reviewed source → builder;
  fail-**closed** admission gate that *blocks* (not just flags) non-conformant
  artifacts.
- **Air-gapped operation.** No internet at build; internal mirrors; **self-hosted
  GitLab CE**; controlled media transfer; everything offline-reproducible.
- **Tool control / qualification.** ISO 26262 puts tools under CM; DO-178C requires
  a Software Life-Cycle Environment Configuration Index (pinned tool versions);
  DO-330 qualifies a tool whose output isn't otherwise verified.

## Coverage matrix

| Need | haw today | Verdict |
|------|-----------|---------|
| Reproducible multi-repo baseline (pinned SHAs) | `haw.lock` pins every repo to a SHA, whole-manifest, byte-identical cross-OS | ✅ **Core strength** |
| Pinned manifest + `sync` (the `repo`/west job) | `haw sync` + `haw.toml`/`haw.lock`, done with a real lockfile | ✅ Better than `repo` (lockfile, no symlinks, Windows) |
| Pre-build drift check, fail-closed | `haw verify` → exit 3 on any divergence from the lock | ✅ **Exact match** (validated: dirty *and* head≠lock → exit 3) |
| Unified build across cmake/gcc/make/npm | `haw build` runs each repo's declared `build =` in parallel | ✅ (validated with gcc; declare the command per repo) |
| Cross-repo feature → PR/MR on GitHub **and** GitLab | `haw change start/request/land`, both forges behind one trait | ✅ Mixed-forge covered |
| Branch protection / no force-push | Enforced by the **forge** (rulesets); haw opens/links the PRs | ✅ (forge-side; haw integrates) |
| Self-hosted GitLab / GitHub Enterprise | Hostname-substring forge detection + explicit `forge =` | ✅ Air-gap-friendly |
| Offline / air-gapped build | Shells out to `git`, `--shared` local mirrors, lock is offline-reproducible | ⚠️ Works if remotes/mirrors are reachable at sync; **no built-in mirror mgmt** |
| Named, authorized baselines (per milestone + role) | `haw pin` / tags give the snapshot; no baseline *record* (id, date, authority) | ⚠️ **Gap** — small addition |
| Toolchain capture (SLECI / tool versions under CM) | `haw evidence` writes a `tool.json` stub | ⚠️ **Gap** — enrich evidence |
| SBOM (CycloneDX + SPDX, NTIA-min, signed) | `haw evidence` bundle exists; SBOM payload deferred | ❌ **Gap → roadmap Phase A** (the wedge) |
| Signed provenance / SLSA / admission gate | `xtask dist` archives + sha256; no signing/provenance | ❌ **Gap → roadmap Phase B** |
| Requirements↔code↔test traceability matrix | Out of haw's declared scope (DOORS/Polarion/LDRA) | ⏸ **Deferred** (roadmap Phase D / services) |
| Non-code CIs (requirements, ARXML, calibration docs) | Out of scope — belongs in the ALM/PLM tool | ⏸ By design |

## Why the scope boundary is a feature (DO-330)

DO-330 qualifies a tool whose output isn't otherwise verified. haw's declared
scope — *"it decides which revision of which repository enters a build; it does
not compile, generate, or verify code"* — deliberately keeps its failure modes
bounded. Adding in-tool code verification (e.g. a real traceability engine) would
**raise haw's own qualification burden**. This is the technical reason `haw-trace`
is deferred to services / the qualification kit, not built into the core.

## Gaps → roadmap

1. **SBOM in `haw evidence`** (Phase A) — CycloneDX + SPDX, NTIA-minimum, built
   from the pinned baseline, orchestrating Syft / cargo-cyclonedx. *The wedge.*
2. **Signed provenance** (`haw-artifact`, Phase B) — SLSA-style attestation +
   signature (cosign/sigstore), fail-closed admission gate over `haw verify`.
3. **Named baselines** (small core addition) — a baseline record: id, date, CI
   version matrix, authorizing role — layered on `haw.lock` + tags.
4. **Toolchain capture** (enrich `haw evidence` `tool.json`) — pinned tool
   versions as the SLECI evidence ISO 26262 / DO-178C expect.
5. **Air-gap ergonomics** — document + smooth the internal-mirror workflow
   (`--shared`, self-hosted forge, offline lock).

## Field notes from the validation run

A prod-like two-ECU stack (`cmake` + `gcc`, pinned to tags) confirmed:
`sync` reproduces the baseline; `haw build` dispatches both toolchains;
`haw verify` returns **exit 3** on both a dirty tree and a head≠lock drift
(fail-closed, as ASPICE/embedded require). One practical note: build **out of
tree** (e.g. `cmake -B build`) — a compiler that writes its binary into the
worktree makes `verify` see the repo as dirty. That is correct behavior, and the
right fix is out-of-tree builds, not relaxing the gate.

## Bottom line

For an embedded SoC shop across automotive / aviation / defense, **haw already
delivers the reproducible multi-repo baseline + drift gate + mixed-forge
changeset workflow those standards demand.** To become *certification evidence*
(the paid tier), it needs the SBOM + signing already on the roadmap. The industry
requirements validate, rather than challenge, the current architecture.
