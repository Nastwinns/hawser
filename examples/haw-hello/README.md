# haw-hello — example subcommand plugin

A minimal [haw plugin](../../docs/PLUGINS.md): the executable `haw-hello` on your
`PATH`. Running `haw hello` dispatches to it — separate process, so it can't crash haw.

## Try it

From the repo root, put this directory on `PATH` for one command:

```console
$ PATH="$PWD/examples/haw-hello:$PATH" haw hello
hello from haw-hello (no workspace here — try inside a haw workspace)
```

Inside a haw workspace (a directory with a `haw.toml`), haw hands the plugin the
workspace context and `haw-hello` reports the root:

```console
$ PATH="$PWD/examples/haw-hello:$PATH" haw hello
hello from haw-hello — workspace at /path/to/your/workspace
```

`--help` is self-describing:

```console
$ PATH="$PWD/examples/haw-hello:$PATH" haw hello --help
haw-hello — example haw subcommand plugin

USAGE:
    haw hello [OPTIONS]

OPTIONS:
    -h, --help       Print this help
        --format json
                     Print a haw.plugin/1 JSON document on stdout

haw runs this as `haw-hello` from PATH and passes the workspace context
as haw.plugin/1 JSON in $HAW_JSON and on stdin.
```

`--format json` emits a machine-readable `haw.plugin/1` document on stdout:

```console
$ PATH="$PWD/examples/haw-hello:$PATH" haw hello --format json
{"schema":"haw.plugin/1","plugin":"hello","greeting":"hello from haw-hello","root":"/path/to/your/workspace"}
```

(The `root` field is empty when run outside a workspace.)

## How it works

haw runs `haw-<name>` from `PATH` and passes the workspace context — the same
`haw.plugin/1` JSON document — two ways:

- in the `HAW_JSON` environment variable, and
- on the plugin's stdin.

`haw-hello` reads `HAW_JSON` to find the workspace `root`. See
[docs/PLUGINS.md](../../docs/PLUGINS.md) for the full contract and a Rust version.

## See also

- The [examples index](../README.md) — all runnable and reading examples.
- [`../governance`](../governance/) — registering plugins on lifecycle phases via
  the `[plugins]` manifest table.
