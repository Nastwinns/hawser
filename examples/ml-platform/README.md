# ml-platform — model + pipeline + serving, pinned together

An ML platform split across three repos that must move as one: the
`data-pipeline` that produces features, the `recommender-model` trained on them,
and the `serving-infra` that deploys the model behind an API. Pinning all three
in `haw.lock` makes "the model that was live in March" a byte-for-byte
reproducible baseline instead of a guess.

This is a *reading* example — the URLs are illustrative, so inspect it with
`--manifest` rather than cloning.

## What it demonstrates

- **Reproducible baseline across three repos.** Pipeline on `v2.1.0`, model on
  `2024.11-rc3`, serving on `v1.7.2`. Commit `haw.lock` and a clone months later
  resolves the identical three trees — the audit record for an ML release.
- **Two stacks sharing the pipeline.** `training` (pipeline + model) and
  `serving` (pipeline + model + infra) reuse the same pipeline repo, no
  duplication.
- **`deps` = land/build order.** `model` depends on `pipeline`; `serving` depends
  on `model`. `haw change land` and dependency-aware builds respect that chain.
- **An overlay for iteration.** `haw sync --overlay bleeding-edge` follows the
  model's `main` branch while pipeline and serving stay pinned.
- **Toolchain-agnostic.** `dvc repro`, `python -m model.train`, `terraform plan`
  — each repo names its own command; `haw` only shells out.

## Inspect it

```console
$ haw tree --manifest examples/ml-platform/haw.toml
```

## The workflow it models

```console
$ haw sync --stack serving        # clone the pinned pipeline + model + infra
$ haw build --group data          # dvc repro the pipeline
$ haw test                        # pytest + model eval + terraform validate
$ haw evidence                    # bundle manifest+lock+status: which SHAs shipped
```

See also: [`../microservices`](../microservices/) and the
[examples index](../README.md).
