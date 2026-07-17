# automotive — ARXML config + shared HAL + ECU apps, gated by MISRA

An automotive fleet wired for governance: AUTOSAR configuration lives in an
`arxml-config` repo pinned to an exact SHA, a shared `hal` is reused across every
ECU, and two ECU app repos (`ecu-powertrain`, `ecu-chassis`) build on both. The
**MISRA C gate is subscribed as a `pre-request` hook**, so a PR that introduces a
MISRA violation is blocked before it opens.

This is a *reading* example — the URLs are illustrative internal-style hosts, so
inspect it with `--manifest` rather than cloning.

## What it demonstrates

- **AUTOSAR ARXML pinned in the lock.** `arxml-config` is pinned to a 40-char SHA
  alongside the code it configures — the config version is part of the auditable
  baseline, not a floating branch.
- **A shared HAL across ECUs.** Both ECU stacks depend on the same `hal`; fix it
  once, both inherit the fix.
- **Toolchain-agnostic builds.** The ARXML repo runs a Vector DaVinci-style
  generate step; the ECUs cross-compile with `arm-none-eabi-gcc`. `haw` bundles
  no compiler — it shells out to each repo's declared `build`/`test`. Swap in
  Green Hills, IAR, Tasking, Wind River Diab, or an EB tresos generator by
  changing one string per repo.
- **MISRA as a merge gate.** The `[plugins]` table subscribes `misra` to
  `pre-request`: `haw` dispatches `haw-misra`, which runs `cppcheck --addon=misra`
  over the fleet's C sources and returns `ok:false` (blocking the PR) on any
  violation. It is **fail-open** — if `cppcheck` isn't installed, it emits a warn
  and lets the PR through, so the gate never blocks adoption on a missing tool.
- **ASPICE traceability on land.** `aspice` fires `post-land` to record
  repo → pinned SHA → process area as evidence.

## Inspect it

```console
$ haw tree --manifest examples/automotive/haw.toml
```

## The workflow it models

```console
$ haw sync --stack powertrain          # clone ARXML config + HAL + powertrain app
$ haw build --group baseline            # generate config, build the HAL
$ haw build                             # cross-build the ECU apps (arm-none-eabi-gcc)
$ haw change start FIX-17 --repos hal,ecu-powertrain
$ haw change request                    # pre-request fires haw-misra: violation => PR blocked
$ haw change land                       # post-land fires haw-aspice traceability
```

## The MISRA gate, standalone

The same plugin runs as a subcommand for a quick local pass:

```console
$ haw misra                             # human summary: files scanned + violation count
$ haw misra --format json               # the raw haw.plugin.report/1 document
```

See [`../../crates/haw-misra`](../../crates/haw-misra/) for the plugin,
[`../../docs/DOMAINS.md`](../../docs/DOMAINS.md) for the standards mapping, and
[`../automotive-pinned`](../automotive-pinned/) for the pinning-focused variant.
