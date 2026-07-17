# Examples

Every example is a real, runnable `haw.toml` in the repo under
[`examples/`](https://github.com/Nastwinns/hawser/tree/main/examples). Clone it (or copy
the manifest), then `haw sync` and explore. The domain examples compose **real public
upstream repos**, so `haw sync` needs network; a few build steps use Docker (noted per
example).

Start with **quickstart**, then jump to whichever domain matches your fleet.

## Learn-by-doing

| Example | What it shows |
|---------|---------------|
| [quickstart](https://github.com/Nastwinns/hawser/tree/main/examples/quickstart) | Two octocat repos + a stack — the whole `sync → status → lock → verify` loop (the [course](learn/02-your-first-stack.md) walks it). |
| [haw-hello](https://github.com/Nastwinns/hawser/tree/main/examples/haw-hello) | A tiny plugin — the `haw <name>` → `haw-<name>` contract in ~20 lines. |
| [governance](https://github.com/Nastwinns/hawser/tree/main/examples/governance) | `[plugins]` lifecycle hooks — SBOM / provenance / gate wired to phases. |

## By domain

| Example | Domain | What it shows |
|---------|--------|---------------|
| [microservices](https://github.com/Nastwinns/hawser/tree/main/examples/microservices) | Backend | 4 services + a shared proto/lib; a feature branched, PR'd, and `land`ed together in dependency order. |
| [ml-platform](https://github.com/Nastwinns/hawser/tree/main/examples/ml-platform) | ML / data | Model + data-pipeline + serving-infra pinned as one reproducible baseline, with stacks + an overlay. |
| [automotive](https://github.com/Nastwinns/hawser/tree/main/examples/automotive) | Embedded / AUTOSAR | ARXML config + shared HAL + two ECU apps, cross-toolchain builds, `[plugins] misra + aspice`. |
| [automotive-pinned](https://github.com/Nastwinns/hawser/tree/main/examples/automotive-pinned) | Embedded | A fully SHA-pinned automotive fleet — the reproducibility/audit baseline. |
| [embedded-bsp](https://github.com/Nastwinns/hawser/tree/main/examples/embedded-bsp) | Embedded | A shared BSP/HAL reused across ECU stacks via overlays. |
| [embedded-real](https://github.com/Nastwinns/hawser/tree/main/examples/embedded-real) | Embedded | **Five real upstreams** (CoreMark, cJSON, Monocypher, libcanard, Mbed-TLS) — all build with one `haw build -j4` (validated). |

## Real build & emulation recipes

For copy-paste `build`/`test` wiring to real toolchains — **Docker cross-compile
(Cortex-M4), FreeRTOS booted under QEMU**, and patterns for EB tresos / Vector / Green
Hills / IAR / Tasking / Zephyr / Renode — see **[Integration recipes](INTEGRATION.md)**.
