# haw-web

The **"html/css basics" starter** plugin — Python 3 **stdlib only**, zero
dependencies. For each repo in the workspace it walks the tree (skipping
`node_modules`, `dist`, `.git`, …) and:

- counts `*.html` files and runs a light **tag-balance sanity check** on each
  (`<!doctype>` present? `<title>` present? are common block tags balanced?),
- lists `*.css` files,
- reports total web-asset size per repo.

```
REPO              HTML   CSS  WARN  SIZE
site                 4     2     1  38.2K
docs                 9     1     0  120.4K
```

Fully **read-only** — it only reads files, never writes or fetches anything.

## Faces

| Invocation | Output |
| --- | --- |
| `haw web` | the human table above |
| `haw web --format json` | a `haw.plugin.report/1` document (`ok:false` if any warnings) |
| cockpit **Plugins** view (`HAW_RENDER=1`) | a `haw.plugin.view/1` panel |

It reads the `haw.plugin/1` context from `$HAW_JSON` (falling back to stdin).

## Install

```console
$ chmod +x haw-web
$ cp haw-web ~/.local/bin/
$ haw web
```

## Try it without a workspace

```console
$ HAW_JSON='{"schema":"haw.plugin/1","root":"'"$PWD"'","repos":[{"name":"demo","path":"'"$PWD"'"}]}' \
    ./haw-web --format json
```
