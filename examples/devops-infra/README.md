# devops-infra — a fleet of real DevOps / infrastructure upstreams

Three genuine public infra projects — a Terraform module, a Helm chart
monorepo, and a Dockerfile app — composed as one `haw` fleet. Every
`build =` / `test =` in [`haw.toml`](haw.toml) was **actually executed with
`haw` and seen to succeed** on this host. `haw sync` clones the real upstreams
over HTTPS with no credentials.

This mirrors [`embedded-real/`](../embedded-real/) but for the infra world:
one `haw build` / `haw test` drives three heterogeneous IaC/container
toolchains at once with a single CI-grade exit code.

## The fleet

| Repo | Domain | build (validated ✓) | test (validated ✓) |
| --- | --- | --- | --- |
| [terraform-aws-vpc](https://github.com/terraform-aws-modules/terraform-aws-vpc) | terraform | `terraform init -backend=false` — downloads AWS provider | `terraform validate` (**"configuration is valid"**) + `fmt -check -recursive` |
| [helm-charts](https://github.com/prometheus-community/helm-charts) | helm | `helm lint charts/kube-state-metrics` — **0 failed** | `helm lint charts/alertmanager charts/kube-state-metrics` — **2 linted, 0 failed** |
| [welcome-to-docker](https://github.com/docker/welcome-to-docker) | docker | `docker build -t haw-welcome .` — real multi-stage image | `hadolint` (containerized) — Dockerfile clean, **exit 0** |

All three build **and** test green: `build ran in 3/3 repos` (exit 0),
`test ran in 3/3 repos` (exit 0).

## Run it

```console
$ mkdir /tmp/devops && cp haw.toml /tmp/devops/ && cd /tmp/devops
$ haw sync --filter=blob:none   # clones all three upstreams (needs network)
$ haw build                     # terraform init / helm lint / docker build
$ haw test                      # terraform validate+fmt / helm lint / hadolint
```

### Captured output (real, this host)

```console
$ haw build
── tf-vpc ──
Terraform has been successfully initialized!
── helm-charts ──
1 chart(s) linted, 0 chart(s) failed
── docker-app ──
naming to docker.io/library/haw-welcome done
build ran in 3/3 repos

$ haw test
── tf-vpc ──
Success! The configuration is valid.
── helm-charts ──
2 chart(s) linted, 0 chart(s) failed
── docker-app ──
test ran in 3/3 repos
```

## Prerequisites

- **Network** — `haw sync` clones the three upstreams from GitHub.
- **terraform** — native (`brew install terraform`) or Docker
  (`hashicorp/terraform:latest`, see the `tf-vpc` comment in `haw.toml`).
- **helm** — native (`brew install helm`) or Docker (`alpine/helm`).
- **Docker** — daemon running, for the `docker build` and the containerized
  `hadolint` lint. `hadolint/hadolint` is pulled on first `haw test`.

Every recipe in this example was run natively for terraform/helm and via Docker
for hadolint; no step is a stub.

See also the [examples index](../README.md).
