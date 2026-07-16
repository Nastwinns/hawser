# haw examples

Hands-on manifests for learning [haw](../README.md) — the multi-repo tool whose
binary is `haw` (crate `hawser`). Start with **quickstart**: it clones real
public repos and runs the whole loop with no credentials. The others are
themed *reading* manifests you inspect with `--manifest`.

## The examples

| Example | Demonstrates | Runnable? | Key commands |
| --- | --- | --- | --- |
| [`quickstart/`](quickstart/) | Full loop on real public repos: shared repo across two stacks, groups, changesets | **Yes — clones over HTTPS, no auth** | `haw sync --stack site`, `haw status`, `haw run '…'`, `haw change start DEMO --repos …` |
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
