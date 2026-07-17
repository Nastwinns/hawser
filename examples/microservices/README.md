# microservices — a feature spanning several backend services

A backend platform split across four services (`gateway`, `billing`, `accounts`,
`notifications`) that all depend on a shared `proto` contract/library repo. This
is the everyday multi-repo case outside embedded: one feature touches several
services at once, and they must be branched, PR'd, and landed together.

This is a *reading* example — the URLs are illustrative GitHub-style hosts, so
inspect it with `--manifest` rather than cloning.

## What it demonstrates

- **A shared repo, many consumers.** `proto` is depended on by every service via
  `deps = ["proto"]`, so `haw change land` merges it **first**, then the services
  that consume it.
- **Heterogeneous toolchains.** Each service declares its own `build`/`test` —
  Go, Rust, npm, Gradle — and `haw` just shells out. No service needs to agree on
  a build system.
- **Groups.** `shared` (proto) vs `service` (the four apps): target a slice with
  `haw build --group service` or `haw test --group shared`.

## Inspect it

```console
$ haw tree --manifest examples/microservices/haw.toml
```

## The workflow it models

```console
$ haw change start FEAT-42 --repos proto,billing,accounts
        # one branch across just the repos this feature touches
$ haw build --group service       # compile the services in parallel
$ haw test                        # run every declared test suite
$ haw change request              # open linked PRs on GitHub
$ haw change status               # aggregated review + CI across the changeset
$ haw change land                 # merge in deps order: proto, then its consumers
```

See also: [`../ml-platform`](../ml-platform/) and the
[examples index](../README.md).
