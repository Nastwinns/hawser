# Domains

`haw` composes, orchestrates, and ships change across many Git repos — in **any**
domain. Nothing in the manifest, the lockfile, the changeset flow, the fleet-wide
build/test, or the governance hooks is specific to one industry. A repo is a repo; a
build is whatever shell command you declare; a PR is a PR.

This page shows how the same loop —

> **compose** (manifest) → **pin** (lock) → **change** (changeset) → **build/test** →
> **govern** (hooks + evidence)

— maps onto five different worlds. Embedded/automotive is one proof point among several,
not `haw`'s identity.

Each section names the pain, then the mapping. Illustrative manifests live under
[`examples/`](https://github.com/Nastwinns/hawser/tree/main/examples).

---

## Backend microservices

**The pain.** A single user-facing feature spans four services and a shared protobuf/lib
repo. You branch each by hand, open four PRs, and try to remember to land them in the
right order without breaking `main`. There is no single artifact that says "these five
SHAs are the feature."

**How `haw` maps:**

- **Manifest / lock.** Declare the services plus the shared `proto` repo; commit
  `haw.lock` so CI and every teammate resolve the *identical* set of SHAs.
- **Changeset.** `haw change start FEAT-42 --repos api,billing,proto` creates one branch
  across exactly the repos the feature touches; `haw change request` opens the linked
  PRs, `haw change status` aggregates review + CI, and `haw change land` merges them in
  `deps` order (proto before its consumers).
- **Build / test.** Each service declares its own `build`/`test` (`cargo`, `go test`,
  `npm test`, `./gradlew`); `haw build -j N` fans them out in parallel and exits nonzero
  if any fails — drop it straight into a pipeline.
- **Govern.** A `pre-request` gate can enforce policy (secret scan, license check) before
  any PR opens.

See [`examples/microservices/`](https://github.com/Nastwinns/hawser/tree/main/examples/microservices).

---

## ML / data platforms

**The pain.** A model repo, the data-pipeline repo that feeds it, and the serving-infra
repo that deploys it drift apart. Reproducing "the model that was live in March" means
guessing which commit of the pipeline produced it.

**How `haw` maps:**

- **Manifest / lock.** Pin the model, pipeline, and serving repos together to exact SHAs.
  The committed lockfile *is* the reproducible baseline — a clone months later resolves
  the same three trees.
- **Stacks / overlays.** A `training` stack and a `serving` stack can share the pipeline
  repo without duplication; an overlay follows `main` for the model while everything else
  stays pinned.
- **Build / test.** Declare `test = "pytest"` on the pipeline, `build = "dvc repro"` or a
  training command on the model, and an infra plan/apply on serving — `haw` shells out,
  it never bundles a toolchain.
- **Govern.** SBOM + provenance hooks record exactly which model, data, and infra SHAs
  shipped together — the audit trail for an ML release.

See [`examples/ml-platform/`](https://github.com/Nastwinns/hawser/tree/main/examples/ml-platform).

---

## Platform / infra

**The pain.** Terraform root modules, reusable submodules, and Helm charts live in
separate repos. A change to a shared module needs coordinated bumps across every consumer,
and "what was deployed" is spread across N repos at N revisions.

**How `haw` maps:**

- **Manifest / lock.** Compose the module and chart repos into one pinned fleet; the lock
  is the deployed-baseline record.
- **Changeset.** Bump a shared module and its consumers on one branch across repos, PR
  them together, and land in dependency order.
- **Build / test.** Declare `test = "terraform validate"` / `"helm lint"` and a
  `build`/plan command per repo; `haw test` runs the whole fleet's checks in parallel.
- **Govern.** `haw verify` (exit 3 on drift) is a CI gate that fails if the checked-out
  tree no longer matches the pinned infra baseline.

See [`examples/microservices/`](https://github.com/Nastwinns/hawser/tree/main/examples/microservices) for the changeset pattern —
the same shape applies to Terraform/Helm repos.

---

## Mobile

**The pain.** An app repo depends on an in-house SDK repo. A feature needs a change in
both, released in lockstep, but the two repos have independent branches, PRs, and CI.

**How `haw` maps:**

- **Manifest / lock.** Pin the app and SDK repos together; the lock guarantees the app
  builds against the exact SDK commit under test.
- **Changeset.** One branch across app + SDK, cross-linked PRs, landed SDK-first via
  `deps`.
- **Build / test.** Declare the Gradle/Xcode/`fastlane` commands per repo; `haw build`
  drives both.
- **Govern.** Signing + SBOM hooks capture what shipped in a release.

---

## Embedded & automotive

**The pain.** A shared HAL/BSP/MCAL is reused across many ECUs; AUTOSAR configuration
lives in ARXML repos that must stay pinned beside the code; and the whole thing has to be
reproducible and auditable for functional-safety qualification.

**How `haw` maps:**

- **Manifest / lock.** Pin BSW/MCAL to exact tags/SHAs and pin the **AUTOSAR ARXML config
  repos in the lock** alongside them — the baseline *is* the audit evidence, byte-for-byte
  reproducible years later.
- **Stacks / overlays.** Multiple ECUs (gateway, body, …) share one BSW+MCAL foundation
  as separate stacks, no duplication; overlays swap a variant's revisions without
  rewriting the repo list.
- **Changeset.** A cross-ECU fix branches across the affected repos and lands in `deps`
  order (base software before the ECU apps that depend on it).
- **Build / test — toolchain-agnostic.** `haw` **shells out to the declared `build`/`test`
  command per repo and bundles no compiler**. That means it drives whatever the repo
  declares:

  - **Vector MICROSAR / DaVinci** configuration + generation steps,
  - **Elektrobit (EB) tresos** generators,
  - **Green Hills** (MULTI / `ccrh`, …), **IAR** (`iccarm`), **Tasking**,
    **Wind River Diab**, and **`arm-none-eabi-gcc`** compilers.

  Each is named in that repo's `build =` / `test =`; `haw` never bundles or requires any
  of them.

- **Govern — standards mapping.** The governance hooks map directly onto the standards
  work:

  | Standard / artifact | How `haw` covers it |
  |---------------------|---------------------|
  | **Automotive SPICE (ASPICE)** | [`haw-aspice`](https://github.com/Nastwinns/hawser/tree/main/crates/haw-aspice) emits repo → pinned SHA → process-area traceability |
  | **MISRA C** | [`haw-misra`](https://github.com/Nastwinns/hawser/tree/main/crates/haw-misra) runs `cppcheck --addon=misra` across the fleet as a `pre-request` gate |
  | **ISO 26262 / DO-178C** | `haw evidence` bundles (manifest + lock + audit + status) plus SBOM + provenance from the governance plugins |
  | **AUTOSAR ARXML** | config repos pinned to exact SHAs in `haw.lock`, versioned with the code they configure |

See [`examples/automotive/`](https://github.com/Nastwinns/hawser/tree/main/examples/automotive) and
[`examples/automotive-pinned/`](https://github.com/Nastwinns/hawser/tree/main/examples/automotive-pinned).

---

## The common thread

Across all five, the moving parts are identical — only the repos and the declared
`build`/`test` commands change:

| Loop stage | Backend | ML / data | Infra | Mobile | Embedded / automotive |
|------------|---------|-----------|-------|--------|-----------------------|
| **Compose** | services + shared proto | model + pipeline + serving | modules + charts | app + SDK | BSW/MCAL + ARXML + ECU apps |
| **Pin** | reproducible feature set | reproducible model baseline | deployed baseline | app↔SDK lockstep | audit baseline |
| **Change** | feature across services | model + pipeline together | module + consumers | app + SDK | cross-ECU fix |
| **Build/test** | `cargo`/`go`/`npm` | `pytest`/`dvc` | `terraform`/`helm` | Gradle/Xcode | Vector/EB/GHS/IAR/gcc |
| **Govern** | policy gate | SBOM/provenance | drift `verify` | signing | ASPICE/MISRA/26262/DO-178C |

One binary, one loop, every domain.
