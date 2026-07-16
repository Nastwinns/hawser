# governance — plugins wired onto lifecycle phases

Shows the `[plugins]` table: out-of-process governance tools that haw fires at
lifecycle phases. haw itself knows nothing about SBOMs, licenses, or signatures —
it just dispatches the `haw-<name>` executable on your PATH when it reaches a
phase. This is a *reading* example; `haw tree --manifest` proves it parses.

## The `[plugins]` table

```toml
[plugins]
haw-compliance = ["pre-build", "pre-request"]
haw-artifact   = ["post-build", "post-land"]
haw-git-gate   = ["post-change-start", "post-land"]
```

Each **key** is a plugin executable (`haw-compliance`, …); each **value** lists
the phases at which haw runs it. Valid phases are the kebab-case hook names:

`pre-sync`, `post-sync`, `pre-lock`, `post-lock`, `post-switch`,
`post-change-start`, `pre-build`, `post-build`, `pre-test`, `post-test`,
`pre-request`, `post-land`.

An unknown phase name is a manifest error, so typos fail fast.

## The governance workflow it models

- **`haw build`** fires `pre-build` → `haw-compliance` runs a license/policy
  check *before* anything compiles; then `post-build` → `haw-artifact` emits an
  **SBOM / attestation** for the freshly built tree.
- **`haw change start`** fires `post-change-start` → `haw-git-gate` sets up
  branch protection / sign-off requirements on the new change branches.
- **`haw change request`** fires `pre-request` → `haw-compliance` re-checks
  before any PR opens.
- **`haw change land`** fires `post-land` → `haw-git-gate` enforces the signed,
  gated merge and `haw-artifact` records the final release attestation.

Net effect: builds are always accompanied by an SBOM, and no change reaches
`main` without passing the compliance gate — enforced by composition, not by
baking policy into haw.

## Inspect it

```console
$ haw tree --manifest examples/governance/haw.toml
examples/governance/haw.toml
└─ release
   ├─ libcore      v3.2.0  (git@github.com:acme/libcore.git)
   └─ service-api  v1.8.0  (git@github.com:acme/service-api.git)
```

`haw dash --demo` renders the fleet cockpit with a governance view (build /
compliance / artifact status per repo) using canned data — no clone or workspace
needed (it must run in a real terminal).

Writing your own plugin? Start from [`../haw-hello`](../haw-hello/) and see
[`../../docs/PLUGINS.md`](../../docs/PLUGINS.md) for the `haw.plugin/1` contract.

See also the [examples index](../README.md).
