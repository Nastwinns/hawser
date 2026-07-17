# haw examples

Hands-on manifests for learning [haw](../README.md) — the multi-repo tool whose
binary is `haw` (crate `hawser`). haw is **domain-agnostic**: these examples span
backend microservices, ML/data platforms, and embedded/automotive — the same
loop in each. Start with **quickstart**: it clones real public repos and runs the
whole loop with no credentials. The others are themed *reading* manifests you
inspect with `--manifest`. See [`../docs/DOMAINS.md`](../docs/DOMAINS.md) for how
the loop maps onto each domain.

## The examples

| Example | Demonstrates | Runnable? | Key commands |
| --- | --- | --- | --- |
| [`quickstart/`](quickstart/) | Full loop on real public repos: shared repo across two stacks, groups, changesets | **Yes — clones over HTTPS, no auth** | `haw sync --stack site`, `haw status`, `haw run '…'`, `haw change start DEMO --repos …` |
| [`embedded-real/`](embedded-real/) | **Real embedded fleet:** five genuine public upstreams (CoreMark, cJSON, Monocypher, libcanard, Mbed-TLS) built + tested with one `haw build`/`haw test` — every command actually executed | **Yes — clones over HTTPS, no auth** | `haw sync`, `haw build -j4`, `haw test` |
| [`devops-infra/`](devops-infra/) | **Real DevOps/infra fleet:** terraform-aws-vpc + Prometheus helm-charts + a Dockerfile app — `terraform init/validate`, `helm lint`, `docker build`+`hadolint`, all build+test 3/3 (every command executed) | **Yes — clones over HTTPS, no auth** | `haw sync --filter=blob:none`, `haw build`, `haw test` |
| [`ml-ai/`](ml-ai/) | **Real AI runtime:** llama.cpp compiled from source into a working `llama-cli` (+ nanoGPT parse-check) — build+test 2/2 (every command executed) | **Yes — clones over HTTPS, no auth** | `haw sync --filter=blob:none`, `haw build -j4`, `haw test` |
| [`mobile/`](mobile/) | **App + SDK pinned in lockstep:** OkHttp SDK **builds + tests for real** via a JDK-21 Docker image; the Now-in-Android app half is an honest pattern (needs Android SDK) | **Partly — OkHttp yes, app needs Android SDK** | `haw sync --stack sdk-only`, `haw build --group sdk`, `haw test --group sdk` |
| [`microservices/`](microservices/) | **Backend** domain: a feature across four services + a shared proto repo; heterogeneous `build`/`test`; land in `deps` order | Reading | `haw tree --manifest …`, `haw change start FEAT --repos …`, `haw change land` |
| [`ml-platform/`](ml-platform/) | **ML / data** domain: model + data-pipeline + serving infra pinned as one reproducible baseline; two stacks; an overlay | Reading | `haw tree --manifest …`, `haw sync --overlay bleeding-edge`, `haw evidence` |
| [`automotive/`](automotive/) | **Embedded** domain: AUTOSAR ARXML + shared HAL + ECU apps; toolchain-agnostic builds; **MISRA gate** as a `pre-request` hook | Reading | `haw tree --manifest …`, `haw misra`, `haw change request` |
| [`automotive-pinned/`](automotive-pinned/) | Fully pinned tags/SHAs for reproducible, auditable baselines (ASPICE/ISO 26262); `deps` land order; per-repo `build =` | Reading + `build` | `haw tree --manifest …`, `haw verify`, `haw evidence` |
| [`embedded-bsp/`](embedded-bsp/) | Zephyr/west-style BSP: fixed checkout paths, pinned vs branch-tracking revs, an overlay, per-repo `build =` | Reading | `haw tree --manifest …`, `haw lock --overlay bleeding-edge` |
| [`governance/`](governance/) | `[plugins]` wired onto lifecycle phases (compliance / SBOM / git-gate) | Reading | `haw tree --manifest …`, `haw dash --demo` |
| [`haw-hello/`](haw-hello/) | Writing a subcommand plugin (`haw hello` → `haw-hello` on PATH) | Yes — runnable plugin | `PATH="$PWD/examples/haw-hello:$PATH" haw hello` |
| [`haw.toml`](haw.toml) | Minimal mixed firmware manifest (two remotes, two stacks, a dev overlay) | Reading | `haw tree --manifest examples/haw.toml` |

## How to run any example

Build the binary once (from the repo root):

```console
$ cargo build --release -p hawser      # produces target/release/haw
```

Put it on your PATH for the session, or call it directly:

```console
$ export PATH="$PWD/target/release:$PATH"   # then `haw …`
# or invoke it explicitly:  ./target/release/haw …
```

**Read-only commands take `--manifest <path>`**, so you can inspect any example
in place without a workspace:

```console
$ haw tree   --manifest examples/embedded-bsp/haw.toml
$ haw status --manifest examples/governance/haw.toml     # per-repo state
```

**To actually clone/build**, copy the manifest into an empty directory first —
`haw sync` writes `haw.lock` and checks repos out next to the manifest:

```console
$ mkdir /tmp/try && cp examples/quickstart/haw.toml /tmp/try/ && cd /tmp/try
$ haw sync --stack site
```

Only [`quickstart/`](quickstart/) points at anonymously-cloneable repos; the
themed manifests use illustrative internal-style URLs and are meant for reading.

## More

- **Try the cockpit in your browser:** <https://nastwinns.github.io/hawser/>
- **Main README:** [`../README.md`](../README.md) — install, demos, full feature tour.
- **Plugin contract:** [`../docs/PLUGINS.md`](../docs/PLUGINS.md).
