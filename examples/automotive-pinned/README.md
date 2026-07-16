# automotive-pinned — reproducible, auditable ECU baselines

An automotive-style fleet where **every repo is pinned** to a tag or an exact
SHA, so the tree is byte-for-byte reproducible for audits (ASPICE, ISO 26262).
This is a *reading* example — the URLs are illustrative internal hosts, so you
inspect it with `--manifest` rather than cloning.

## What it demonstrates

- **Pinning for reproducibility.** `bsw` is on tag `R23-11.2`, `mcal` on a full
  40-char SHA, and the ECU apps on release tags (`v2.4.0`, `v1.9.3`). Nothing
  floats. Commit `haw.lock` beside the manifest and a clone years later resolves
  the identical tree — the baseline *is* the audit evidence.
- **Two stacks sharing a base.** `gateway-ecu` and `body-ecu` both build on the
  same `bsw` + `mcal` foundation, then add their own ECU app. Fix the base once,
  both ECUs inherit it.
- **Groups.** `baseline` (bsw, mcal) vs `ecu` (the two apps) — target a slice
  with `haw build --group baseline` or `haw run --group ecu '…'`.
- **`deps` = land order.** Each ECU app declares `deps = ["bsw", "mcal"]`, so
  `haw change land` merges the base before the apps that depend on it.
- **`build =` per repo.** Each repo carries its toolchain command (cmake / make),
  so `haw build` compiles the whole fleet in parallel without haw knowing
  anything about the build systems.

## Inspect it

```console
$ haw tree --manifest examples/automotive-pinned/haw.toml
examples/automotive-pinned/haw.toml
├─ gateway-ecu
│  ├─ bsw          R23-11.2  (git@gitlab.company.com:platform/bsw.git)
│  ├─ mcal         8f4e2a91c6b3d7e0f1a5b9c8d2e6f3a7b1c4d8e2  (git@gitlab.company.com:platform/mcal.git)
│  └─ ecu-gateway  v2.4.0  (git@gitlab.company.com:platform/ecu-gateway.git)
└─ body-ecu
   ├─ bsw       R23-11.2  (git@gitlab.company.com:platform/bsw.git)
   ├─ mcal      8f4e2a91c6b3d7e0f1a5b9c8d2e6f3a7b1c4d8e2  (git@gitlab.company.com:platform/mcal.git)
   └─ ecu-body  v1.9.3  (git@gitlab.company.com:platform/ecu-body.git)
```

## The workflow it models

Against a real (credentialed) clone of these repos you would:

```console
$ haw sync --stack gateway-ecu     # clone the pinned baseline, write haw.lock
$ haw build --group baseline        # compile bsw + mcal with their cmake/make cmds
$ haw build                         # then the ECU apps, in dependency order
$ haw evidence                      # bundle manifest + lock + status for the audit
$ haw verify                        # CI gate: fail (exit 3) if disk drifts from lock
```

`haw verify` is the auditable contract: if any repo drifts from the pinned SHA,
it exits non-zero — the baseline is either intact or the build fails.

See also: [`../embedded-bsp`](../embedded-bsp/) (branch-tracking BSP),
[`../governance`](../governance/) (compliance plugins), and the
[examples index](../README.md).
