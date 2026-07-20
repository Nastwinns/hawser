# haw-k8s

A POSIX-`sh` **DevOps starter** plugin that finds and validates Kubernetes
manifests. For each repo it searches `k8s/`, `deploy/`, `deployment/`, and
`manifests/` for `*.yaml` / `*.yml`, and — when `kubectl` is present —
validates each with:

```
kubectl apply --dry-run=client -f <file>
```

`--dry-run=client` is **offline**: it never contacts a cluster and **never
applies anything**. With no `kubectl` it simply reports which manifests it
found.

```
REPO             MANIFESTS
api              3 file(s): 3 valid, 0 invalid
web              2 file(s): 1 valid, 1 invalid
worker           no manifests
```

Fully **read-only** — this plugin never applies, deletes, or connects to any
cluster.

## Faces

| Invocation | Output |
| --- | --- |
| `haw k8s` | the human summary above |
| `haw k8s --format json` | a `haw.plugin.report/1` document (`ok:false` if any manifest is invalid) |
| cockpit **Plugins** view (`HAW_RENDER=1`) | a `haw.plugin.view/1` panel |

## Install

```console
$ chmod +x haw-k8s
$ cp haw-k8s ~/.local/bin/
$ haw k8s
```
