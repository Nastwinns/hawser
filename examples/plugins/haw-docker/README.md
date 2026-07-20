# haw-docker

A POSIX-`sh` **DevOps starter** plugin. For each repo in the workspace it looks
for a `Dockerfile` or a compose file (`compose.yaml`, `compose.yml`,
`docker-compose.yml`) and reports container status:

```
REPO             ASSETS                   DETAIL
api              Dockerfile               hadolint:ok image:present
web              Dockerfile,compose.yaml  hadolint:2-issues image:absent
worker           none                     no container assets
```

**Degrades gracefully:** with no `docker`/`hadolint` installed it still reports
which repos carry container assets. Fully **read-only** — it never builds,
pulls, tags, or runs a container.

## Optional tools (never required)

| Tool | Used for |
| --- | --- |
| `hadolint` | lint each `Dockerfile` |
| `docker` | check whether a local image named after the repo exists (`docker images`) |

## Faces

| Invocation | Output |
| --- | --- |
| `haw docker` | the human summary above |
| `haw docker --format json` | a `haw.plugin.report/1` document |
| cockpit **Plugins** view (`HAW_RENDER=1`) | a `haw.plugin.view/1` panel |

## Install

```console
$ chmod +x haw-docker
$ cp haw-docker ~/.local/bin/
$ haw docker
```
