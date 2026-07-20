# haw.nvim

A small, **dependency-free** Neovim plugin that runs [haw](../../README.md)
(hawser) from inside the editor. It simply **shells the `haw` binary** on your
PATH — there is no server and no daemon. Where useful it parses
`--format json` output; otherwise it drops you into a terminal split.

## Commands

| Command | What it does |
| --- | --- |
| `:HawSync` | run `haw sync` and echo the result via `vim.notify` |
| `:HawStatus` | run `haw status` into a scratch buffer (`q` to close) |
| `:HawDash` | open `haw dash` (the TUI cockpit) in a terminal split |
| `:HawFleet` | list the fleet (repo / branch / state) in a scratch buffer, parsed from `haw status --format json`, falling back to plain `haw status` text |

Scratch buffers are throwaway (`nofile`, wiped on close) and bind `q` to close.

## Requirements

- Neovim 0.7+ (uses `nvim_create_user_command` and `vim.keymap.set`).
- The `haw` binary on your `PATH`. Nothing else — no Lua dependencies.

## Install

### lazy.nvim

```lua
{
  "yourfork/haw.nvim",           -- or a local dir: dir = "~/keelson/examples/nvim"
  cmd = { "HawSync", "HawStatus", "HawDash", "HawFleet" },
  config = function()
    require("haw").setup({
      -- bin = "haw",             -- path to the haw binary (default "haw")
      -- dash_split = "split",    -- or "vsplit" for :HawDash
    })
  end,
}
```

### packer.nvim

```lua
use({
  "yourfork/haw.nvim",
  config = function()
    require("haw").setup()
  end,
})
```

### Native (`:packadd`) — no plugin manager

Copy or symlink this directory into a `pack` path, then load it:

```console
$ mkdir -p ~/.local/share/nvim/site/pack/haw/opt
$ ln -s "$PWD/examples/nvim" ~/.local/share/nvim/site/pack/haw/opt/haw.nvim
```

```lua
-- in init.lua
vim.cmd("packadd haw.nvim")
require("haw").setup()
```

(Put it under `pack/haw/start/` instead of `opt/` to load it automatically
without `:packadd`.)

## Configuration

```lua
require("haw").setup({
  bin = "haw",          -- path/name of the haw binary
  dash_split = "split", -- ":HawDash" split direction: "split" or "vsplit"
})
```

## How it works

Every command is a thin wrapper around `vim.fn.systemlist({ "haw", ... })`.
`:HawFleet` decodes `haw status --format json` with `vim.json.decode` and prints
one line per repo; if that output isn't JSON it falls back to the plain text of
`haw status`, so the command is robust across haw versions. No background jobs,
no RPC — quit Neovim and nothing lingers.
