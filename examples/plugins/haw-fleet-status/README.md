# haw-fleet-status

A tiny POSIX-`sh` **starter** plugin: a compact per-repo health panel for a haw
workspace. For every repo it prints one line — **branch**, **clean/dirty(N)**,
and **ahead/behind** vs. the upstream:

```
REPO             BRANCH               STATE      SYNC
api              main                 clean      +0/-0
web              feat/login           dirty(3)   +2/-0
```

Pure `git`, **zero dependencies**, fully read-only. A friendly "hello world
that's actually useful" for the haw plugin protocol.

## Faces

| Invocation | Output |
| --- | --- |
| `haw fleet-status` | the human panel above |
| `haw fleet-status --format json` | a `haw.plugin.report/1` document |
| cockpit **Plugins** view (`HAW_RENDER=1`) | a `haw.plugin.view/1` panel |

It reads the `haw.plugin/1` context from `$HAW_JSON` (falling back to stdin).
Outside a workspace it prints a friendly no-op.

## Install

```console
$ chmod +x haw-fleet-status
$ cp haw-fleet-status ~/.local/bin/     # anywhere on PATH
$ haw fleet-status                      # haw dispatches haw-<name>
```

## Try it without a workspace

```console
$ HAW_JSON='{"schema":"haw.plugin/1","root":"/tmp/ws","repos":[{"name":"demo","path":"'"$PWD"'"}]}' \
    ./haw-fleet-status --format json
```
