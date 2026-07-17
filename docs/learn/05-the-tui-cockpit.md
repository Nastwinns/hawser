# 5. The TUI cockpit

Everything you've done at the command line has a home: a live, keyboard-driven cockpit —
your mission control for the whole fleet. It's in the spirit of `k9s` or `htop`: a
full-screen terminal UI where you *see* the fleet, *drill* into any repo or PR or CI run,
and *act* — merge, approve, checkout — without ever leaving the terminal.

In this chapter you'll take a guided tour. Open it and follow along.

## 1. Open it

Run `haw` with **no subcommand**:

```bash
haw
```

Just want to explore without a real workspace or network? Use the built-in demo
controller — it's populated with canned repos, PRs, and CI runs, so every view has
something to show:

```bash
haw dash --demo
```

Either way you land on the **fleet grid** — the cockpit's home screen.

## 2. Read the fleet grid

```text
 haw ▸ ~/work/gateway ───────────────────────── stack: gateway   lock: ✓   repos: 3/3
──────────────────────────────────────────────────────────────────────────────────────
   REPO        BRANCH ▲      HEAD       DIRTY   DRIFT   ↑ / ↓    MERGE
   kernel      v6.1.2        a1b2c3d4     ·       ·      0 / 0     —
 ◉ hal         main          9f8e7d6c    yes      ·      2 / 0     —
▸⚠ app-mqtt    release/2.x   4d5e6f7a     ·      DRIFT   0 / 5     —
──────────────────────────────────────────────────────────────────────────────────────
 hal  ›  path hal/   branch main (ahead 2)   dirty   locked 9f8e7d6c   grp firmware
──────────────────────────────────────────────────────────────────────────────────────
 [s]ync [f]iles [x]shell [!]exec [/]filter [p]roblems [:]cmd [Enter]drill [?]help
```

This is `haw status`, alive. Each row is a repo; the columns are the same ones you already
know — branch, HEAD, dirty, drift, ahead/behind. The `▸` is your cursor, `◉` marks a
selected repo, and `⚠` flags a problem (like the drift on `app-mqtt`). The grid
**auto-refreshes** about every 5 seconds while idle — never while you're typing — and
`F5` / `Ctrl-R` refresh on demand.

Move with `↑`/`↓` (or `k`/`j`), exactly like Vim.

## 3. Drill in — the core loop is read → drill → act

Put the cursor on a repo and press **`Enter`**. You drill into that repo's Git detail:
branch, SHA, working-tree status, recent log, diffstat, remotes. Press `Esc` (or `b`) to
come back up a level.

That's the rhythm of the whole cockpit: **read** the grid → **drill** into a thing → **act**
on it → back out. You're never more than a keystroke from detail or from action.

## 4. Act on the fleet from the home row

Single keys on the cursor row do things. The essentials:

| Key | Does |
|-----|------|
| `s` | **sync** — the marked repos if any, else the cursor repo, else the stack |
| `f` | browse the repo's **files** (local; `R` switches to the forge API) |
| `x` | drop into a **shell** in that repo |
| `!` | run one **command** (`exec`) in the repo |
| `/` | fuzzy **filter** the grid live — `/knl` narrows to `kernel` |
| `p` | **problems-only** view — just the repos that need attention |
| `Space` | **mark** / unmark the cursor repo (`◉`) |
| `r` | **run** a command — across the marked repos if any, else the whole fleet |

> **Tip:** Marks are the cockpit's superpower. Press `Space` on a few repos, then `s`
> (sync) or `r` (run) acts on *just that set*. It's how you do a surgical fleet operation
> without touching a manifest.

## 5. The network views — PRs, CI, and acting on them

Three keys open fleet-wide network views. They load on demand (nothing hits the network
until you ask):

- **`m`** — every open **PR/MR** across the fleet.
- **`i`** — recent **CI** runs.
- **`v`** — the **governance** view (plugins, SBOM, findings — more in the next chapter).

Inside the PR/MR or CI views, `Enter` drills into detail — a PR's reviewers and checks, or
a CI run's jobs and live progress. And from there, the *action* keys — each **confirm-gated**
with a `y/n` so you never merge by fat-finger:

| Key | Does |
|-----|------|
| `M` | **merge** the PR/MR on its forge |
| `A` | **approve** the PR/MR |
| `C` | **checkout** the PR branch locally as `haw-pr-<n>` |
| `o` | **open** the row in your browser |

So the full cross-forge flow from Chapter 4 — see PRs, approve, merge — is right here,
keyboard-only.

## 6. The command bar — one language for CLI and TUI

Press **`:`** to open the command bar. Its verbs *mirror the CLI you already learned*, and
the status line echoes the exact command each one runs — so the cockpit doubles as a way
to discover the CLI:

```text
:sync              sync the current stack
:grep TODO         fleet-wide grep
:switch platform   switch to another stack
:change land FEAT-42
:theme nord        change the skin live
```

Learn one, know both. `:name` also jumps the cursor to a repo by name.

## 7. Two more views, and themes

- **`P`** — the **Plugins** view: every available plugin; `Enter` runs one and shows its
  output in a panel. (Chapter 6 is all about these.)
- **`E`** — the **Errors** view: a rolling log of this session's failures, so a transient
  error never scrolls away before you can read it.

**Themes.** Six built-in skins — `catppuccin` (default), `dracula`, `nord`, `gruvbox`,
`solarized`, `monochrome`. Switch live with `:theme nord`, set one at startup with
`HAW_THEME=nord haw`, and `NO_COLOR` forces `monochrome`.

Press **`?`** any time for the help overlay, and `q` (or `Ctrl-C`) to quit.

> **Tip:** Every heavy action runs on a background worker, so the UI never freezes while a
> sync, a fetch, or a forge call is in flight. Keep navigating.

## Recap

- Bare `haw` (or `haw dash --demo`) opens the cockpit — a live `haw status` you can act on.
- The loop is **read → drill (`Enter`) → act → back (`Esc`)**.
- Fleet keys: `s` sync, `f` files, `x` shell, `!` exec, `/` filter, `p` problems,
  `Space` mark, `r` run.
- Network views: `m` PRs, `i` CI, `v` governance; then `M`/`A`/`C` merge/approve/checkout
  (confirm-gated).
- `:` is a command bar mirroring the CLI; `P` plugins, `E` errors; six themes via
  `:theme` / `HAW_THEME`.

## Next

You saw a Plugins view — let's find out what plugins are, use one, and scaffold your own →
[6. Plugins and extending](06-plugins-and-extending.md).
