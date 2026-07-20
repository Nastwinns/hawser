<!-- Maintainer note: the reaction GIFs in this course (class="meme") are
     hotlinked directly from Giphy (external URLs). That's intentional and
     fine — they render via <img>. If Giphy ever changes a URL, just swap it. -->

# 8. Build a plugin — and let Claude write your commits

<img class="chapter-illus" src="../assets/img/pair-programming.svg" alt="Pair-programming with Claude to build a haw plugin">

*You bring the workspace, Claude brings the prose — pair-programming your commits and PRs.*

You've composed a fleet, pinned it, lived in the cockpit, shipped changesets, and gated it
in CI. Now the fun part: **you're going to extend haw yourself**. And by the end of this
chapter you'll have taught an AI assistant — Claude — to read your whole fleet and write
your commits and pull-request text for you.

This chapter assumes nothing. If you've never written a haw plugin, don't know what MCP is,
and have never touched the plugin protocol, you're in exactly the right place. We'll define
every term the first time it appears, walk every command, and show you the output you should
expect at each step.

We'll build one real plugin — **`haw-commit-ai`** — and grow it in **two levels**:

- **Level 1 — the foundation (single repo).** Learn the plugin mechanics and the MCP
  handshake on one repo at a time. Honest and simple. At this level Claude already sees a
  single repo's diff on its own, so we're mostly teaching it to speak haw's protocol
  cleanly.
- **Level 2 — the cross-repo power (changeset-wide).** The payoff. Claude on its own
  **cannot** see a *fleet-wide changeset* that spans several repos. haw can. We hand Claude
  the combined cross-repo diff and let it write **one** coherent pull request that narrates
  every repo together. This is the thing neither Claude nor a single-repo tool like lazygit
  can do alone.

<div class="objectives">

**In this chapter, you'll learn to…**

- Understand what a haw plugin actually is, and how haw finds and runs it.
- Read the <code>haw.plugin/1</code> context that haw hands every plugin, field by field.
- Scaffold a working plugin with <code>haw plugins new</code>.
- Emit the two machine shapes haw understands: a <code>haw.plugin.report/1</code> for <code>--format json</code> and a <code>haw.plugin.view/1</code> panel for the cockpit's Plugins view (<code>7</code>).
- Understand what MCP is, and turn the same script into an <strong>MCP server</strong> so Claude can call small, safe tools.
- <strong>Level 2:</strong> hand Claude the <em>combined</em> cross-repo diff and let it draft a single fleet-wide PR — the cross-repo story no single-repo tool can tell.

</div>

---

# Level 1 — the foundation (single repo)

This is the base you build everything on: one repo at a time, and every concept you need to
understand the plugin protocol and the MCP handshake. It's deliberately simple. Get
comfortable here, then Level 2 unlocks the cross-repo power.

Think of Level 1 as the training wheels: useful, honest, and on purpose not yet magical.
Stay through to Level 2 — that's where it takes off.

## 1. What a haw plugin actually is

Let's start from zero. **A haw plugin is nothing more than an executable program named
`haw-<name>` that lives somewhere on your `PATH`.** That's the whole idea. There is no
plugin registry to sign up for, no dynamic library to load, no special API to link against.

haw follows the same convention as `git`, `cargo`, and `kubectl`: when you type a subcommand
it doesn't recognize, it looks for a matching executable and runs it. Here's the flow in
words:

1. You type `haw commit-ai`.
2. haw checks its built-in subcommands. `commit-ai` isn't one of them.
3. haw searches every directory on your `PATH` for an executable named `haw-commit-ai`.
4. It finds one, runs it as a **separate process**, and forwards your arguments to it.
5. Whatever that program prints becomes the output; whatever exit code it returns becomes
   haw's exit code.

So `haw commit-ai` is really just "run the program `haw-commit-ai`, and hand it some
context about my workspace." This is called **PATH dispatch**, and it's why you can ship
`haw-jira`, `haw-sbom`, or `haw-whatever` without ever touching haw's source code. A broken
plugin can't crash haw, because it runs in its own process.

You can see exactly which directories haw scans:

```bash
haw plugins path      # prints the PATH directories haw searches for haw-* binaries
```

We're going to build a plugin called **`haw-commit-ai`**. It has two faces, both from a
single script:

- As an ordinary plugin (`haw commit-ai`) it drafts commit messages and PR text from your
  diffs.
- As an **MCP server** it lets **Claude** read your workspace and diffs and write the real
  commit and PR text itself — safely.

We'll do the plain plugin first, then add the MCP face.

## 2. The context haw hands every plugin: `haw.plugin/1`

When haw runs your plugin, it doesn't just launch a blind program. It hands the plugin a
**context**: a JSON document that describes your workspace. This document has a schema name,
**`haw.plugin/1`**, and it is the contract between haw and every plugin.

**Where does it come from?** haw provides the same JSON in two places, so you can read
whichever is convenient:

- the **`HAW_JSON`** environment variable, and
- the plugin's **standard input (stdin)**.

The content is identical in both. The environment variable is usually easier because reading
it never blocks.

**What's inside?** Here's a real example of the context inside a workspace:

```json
{
  "schema": "haw.plugin/1",
  "root": "/path/to/workspace",
  "stack": "gateway",
  "repos": [
    { "name": "kernel", "path": "/path/to/workspace/kernel", "rev": "v6.1.2", "groups": ["firmware"] },
    { "name": "hal",    "path": "/path/to/workspace/hal",    "rev": "main",   "groups": ["firmware"] }
  ]
}
```

Let's read it field by field:

- **`schema`** — always `"haw.plugin/1"`. It tells you which contract version you're
  looking at.
- **`root`** — the absolute path to the workspace root (the directory that holds your
  manifest). Everything the plugin writes should stay inside this.
- **`stack`** — the name of the active stack (the named selection of repos you're working
  with). Here it's `"gateway"`.
- **`repos`** — the list of repositories in play. Each entry has:
  - **`name`** — the repo's short name (`kernel`, `hal`).
  - **`path`** — its absolute on-disk location. This is the important one: to run `git diff`
    or `git commit` on a repo, you shell out into this `path`.
  - **`rev`** — the pinned revision (a tag like `v6.1.2` or a branch like `main`).
  - **`groups`** — the groups the repo belongs to (`["firmware"]`).

**Run outside a workspace**, the context degrades to just `{"schema": "haw.plugin/1"}` — no
`root`, no `repos`. A well-behaved plugin checks whether `root` and `repos` are present and
does something sensible when they're absent.

Here is how you read that context in Python, step by step. Read the environment variable
first; if it's empty, fall back to stdin; if there's nothing at all, return the minimal
context so the plugin never crashes:

```python
import json, os, sys

def read_context() -> dict:
    raw = os.environ.get("HAW_JSON", "")       # 1. prefer the env var (never blocks)
    if not raw and not sys.stdin.isatty():     # 2. fall back to stdin if it's piped in
        raw = sys.stdin.read()
    if not raw:                                # 3. nothing at all → minimal context
        return {"schema": "haw.plugin/1"}
    try:
        ctx = json.loads(raw)                  # 4. parse the JSON
    except ValueError:
        return {"schema": "haw.plugin/1"}      # 5. malformed → degrade gracefully
    return ctx if isinstance(ctx, dict) else {"schema": "haw.plugin/1"}
```

Once you have `ctx`, everything else is ordinary shell work: `ctx["repos"]` gives you each
repo's on-disk `path`, and `git diff` / `git commit` are just subprocess calls into that
path.

## 3. The three output shapes a plugin can print

A plugin can print three different kinds of output, depending on how it's called. You don't
have to support all three, but a good one does:

1. **Plain text** — the default. When someone runs `haw commit-ai` in a terminal, print
   friendly human-readable text.
2. **A machine report** — when called with `--format json`, print a
   **`haw.plugin.report/1`** document: `{schema, plugin, ok, summary, findings}`. Tools and
   CI parse this instead of scraping human text.
3. **A cockpit panel** — when haw wants to render your plugin inside the TUI cockpit, it
   sets the environment variable `HAW_RENDER=1` and puts `"intent": "render"` in the
   context. Your plugin then prints a **`haw.plugin.view/1`** document:
   `{schema, title, lines[]}`. haw draws those lines in the cockpit's Plugins view (press
   `7`).

The three JSON schemas live in
[`schemas/`](https://github.com/Nastwinns/hawser/tree/main/schemas) — they're the source of
truth for every field.

## 4. Scaffold the plugin

You don't have to write any of this from a blank file. haw generates a runnable skeleton
that already implements the contract for you.

**Prerequisite:** you need Python 3 installed. Check it:

```bash
python3 --version      # any recent Python 3.x is fine
```

Now scaffold:

```bash
haw plugins new commit-ai --lang python
```

```console
created ./haw-commit-ai/haw-commit-ai   (executable, python3)
created ./haw-commit-ai/README.md
next:
  chmod is already set — drop it on PATH:
    PATH="$PWD/haw-commit-ai:$PATH" haw commit-ai
```

Two files land in a new `./haw-commit-ai/` directory:

- **`haw-commit-ai`** — the plugin executable itself (a Python script with a
  `#!/usr/bin/env python3` shebang, already marked executable).
- **`README.md`** — notes for the plugin.

The scaffold already reads `$HAW_JSON`, handles `--help` and `--format json`, and emits a
`haw.plugin.report/1`. It's a correct, working plugin as-is. In the next sections we'll
replace its body with our MCP-capable version.

<div class="callout note">

**Why Python?** Because the MCP SDK we'll use for the Claude side is Python-first. The
plugin *face* stays zero-dependency (standard library only); only the `--mcp` face needs
one extra package, which we install later with `pip install mcp`.

</div>

## 5. The plugin faces: human text, JSON report, and cockpit panel

Let's build the plugin. We'll introduce it piece by piece so nothing is a mystery, then show
you the full script.

First, a few small helpers. `context_repos` pulls the repo list out of the context safely,
and `_run` is a thin wrapper around running a shell command and capturing its output:

```python
import json, os, subprocess, sys

def context_repos(ctx):
    r = ctx.get("repos")
    return [x for x in r if isinstance(x, dict)] if isinstance(r, list) else []

def _run(cmd, cwd=None):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, check=False)
    return p.returncode, p.stdout, p.stderr

def repo_diff_text(path):                        # staged + unstaged changes vs HEAD
    rc, out, _ = _run(["git", "-C", path, "diff", "HEAD"], cwd=path)
    return out
```

Next, `changeset_repos` figures out *which* repos to act on. It asks haw which repos the
current changeset touched; if that comes back empty, it falls back to any repo with dirty
(uncommitted) changes:

```python
def changeset_repos(ctx):                        # touched repos, else dirty repos
    root, repos = ctx.get("root"), context_repos(ctx)
    if root:
        rc, out, _ = _run(["haw", "change", "status", "--format", "json"], cwd=root)
        if rc == 0 and out.strip():
            try: data = json.loads(out)
            except ValueError: data = {}
            names = {r.get("name") for r in data.get("repos", []) if isinstance(r, dict)}
            touched = [r for r in repos if r.get("name") in names]
            if touched: return touched
    dirty = []
    for r in repos:
        rc, out, _ = _run(["git", "-C", r["path"], "status", "--porcelain"], cwd=r["path"])
        if out.strip(): dirty.append(r)
    return dirty
```

Now the two machine outputs. `emit_report` prints the `haw.plugin.report/1` document for
`--format json`, and `emit_view` prints the `haw.plugin.view/1` panel for the cockpit:

```python
def emit_report(ctx):
    repos = changeset_repos(ctx) or context_repos(ctx)
    findings = [{"level": "info", "message": f"{r['name']}: draft a commit"} for r in repos]
    print(json.dumps({"schema": "haw.plugin.report/1", "plugin": "commit-ai",
                       "ok": True, "summary": f"{len(repos)} repo(s)", "findings": findings}, indent=2))

def emit_view(ctx):
    repos = changeset_repos(ctx) or context_repos(ctx)
    lines = [f"{r['name']:<16} draft a commit" for r in repos] or ["nothing to commit"]
    print(json.dumps({"schema": "haw.plugin.view/1",
                      "title": "commit-ai — proposed commits", "lines": lines}))
```

That's the entire plugin face — the part that needs no external packages at all. The MCP
face comes next.

## 6. What MCP is, and why it matters here

Before we write the MCP face, let's define MCP, because you can't wire up what you don't
understand.

**MCP (Model Context Protocol) is a standard way for an AI assistant to call tools you
expose.** In three sentences:

1. It's a simple protocol spoken over stdio (standard input/output) using JSON-RPC messages
   — your program reads requests on stdin and writes responses on stdout.
2. Your program advertises a set of **tools** (named functions with typed arguments), and an
   AI assistant like Claude can call them and read the results.
3. That's it: MCP is the bridge that lets Claude *do things in your world* — read a diff,
   commit a repo — instead of only chatting about it.

**Why it matters here:** haw knows about your whole fleet — every repo, every path, every
changeset. If we expose that knowledge as MCP tools, Claude can call them to read your diffs
and write accurate commit messages and PR text. Claude stops guessing and starts working
from the real diff.

The good news: you don't implement the JSON-RPC wire protocol yourself. The official MCP SDK
ships a helper called **FastMCP** that turns a plain Python function into a tool with one
decorator. You write normal functions; FastMCP handles the protocol.

## 7. The MCP face: the tools Claude will call

Here's the `run_mcp()` function. It imports FastMCP (failing gracefully if the package isn't
installed), creates a server, and registers each tool with the `@mcp.tool()` decorator. The
docstring of each function is what Claude sees as the tool's description, so we write them
clearly.

Two small guards are defined first: `_repo_path` looks up a repo's on-disk path by name, and
`_within_root` ensures any write stays inside the workspace `root`:

```python
def run_mcp():
    try:
        from mcp.server.fastmcp import FastMCP
    except ImportError:
        sys.stderr.write("haw-commit-ai --mcp needs the MCP SDK: pip install mcp\n")
        return 1
    mcp = FastMCP("haw-commit-ai")

    def _repo_path(ctx, repo):
        return next((r.get("path") for r in context_repos(ctx) if r.get("name") == repo), None)

    def _within_root(root, path):                 # path-guard: writes stay inside root
        if not root or not path: return False
        root_abs, path_abs = os.path.realpath(root), os.path.realpath(path)
        return path_abs == root_abs or path_abs.startswith(root_abs + os.sep)
```

Now the five Level 1 tools. Read the docstrings — that's what Claude reads too:

```python
    @mcp.tool()
    def haw_context() -> dict:
        """Workspace root, current stack, and repos (name, path, rev, groups)."""
        ctx = read_context()
        return {"root": ctx.get("root"), "stack": ctx.get("stack"), "repos": context_repos(ctx)}

    @mcp.tool()
    def repo_diff(repo: str) -> str:
        """The staged+unstaged git diff for a repo — see what changed."""
        path = _repo_path(read_context(), repo)
        return repo_diff_text(path) if path else f"no repo named {repo!r}"

    @mcp.tool()
    def changeset_repos_tool() -> list:
        """Repos touched by the current changeset, else the dirty repos."""
        return changeset_repos(read_context())

    @mcp.tool()
    def write_commit(repo: str, message: str) -> str:
        """git commit -m in a repo. Path-guarded to the workspace root."""
        ctx = read_context(); path = _repo_path(ctx, repo)
        if not _within_root(ctx.get("root"), path):
            return f"refused: {repo!r} is outside the workspace root."
        rc, out, err = _run(["git", "-C", path, "commit", "-m", message], cwd=path)
        return f"committed {repo}:\n{out}" if rc == 0 else f"commit failed:\n{err or out}"

    @mcp.tool()
    def draft_pr(repo: str, title: str, body: str, submit: bool = False) -> str:
        """Return PR text. Dry by default — never pushes unless submit=True."""
        text = f"# {title}\n\n{body}"
        if not submit:
            return text + "\n\n(dry run — pass submit=True to run `haw change request`)"
        ctx = read_context()
        rc, out, err = _run(["haw", "change", "request", "--title", title, "--body", body],
                            cwd=ctx.get("root"))
        return f"{text}\n\n[request: {'ok' if rc == 0 else 'failed'}]\n{out or err}"

    mcp.run()
    return 0
```

Here's what each tool does and why it exists:

- **`haw_context()`** — hands Claude the workspace shape: root, stack, and repos. This is how
  Claude learns your fleet exists.
- **`repo_diff(repo)`** — returns one repo's diff so Claude reads exactly what changed
  before writing about it.
- **`changeset_repos_tool()`** — tells Claude which repos are in play right now.
- **`write_commit(repo, message)`** — actually commits, but only inside the workspace root
  (the path-guard refuses anything outside).
- **`draft_pr(repo, title, body)`** — returns PR text; **dry by default**, so it never
  pushes anything unless you explicitly pass `submit=True`.

Finally, `main()` ties every face together — help, MCP, JSON report, cockpit render, and the
default human text:

```python
def main():
    args = sys.argv[1:]
    if "-h" in args or "--help" in args:
        print("haw-commit-ai — draft commits/PRs; --mcp to serve Claude"); return 0
    if "--mcp" in args: return run_mcp()
    ctx = read_context()
    if "--format" in args and "json" in args: emit_report(ctx); return 0
    if os.environ.get("HAW_RENDER") == "1" or ctx.get("intent") == "render":
        emit_view(ctx); return 0
    repos = changeset_repos(ctx) or context_repos(ctx)
    print(f"haw-commit-ai — {len(repos)} repo(s). Run with --mcp to let Claude write.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
```

<div class="callout note">

The listing above is the **Level 1** version. The shipped
[`examples/plugins/haw-commit-ai/haw-commit-ai`](https://github.com/Nastwinns/hawser/tree/main/examples/plugins/haw-commit-ai)
is the fully-commented version (with a real conventional-commit skeleton and a PR-body
template) and *also* carries the **Level 2** cross-repo tools we add below. Both pass
`python3 -m py_compile` and run with zero dependencies in plugin mode.

</div>

## 8. Run the plugin — no MCP, no Claude yet

Let's prove it works as a plain plugin first. Make the file executable and put its directory
on your `PATH` for the command:

```bash
chmod +x haw-commit-ai
PATH="$PWD:$PATH" haw commit-ai               # human draft
PATH="$PWD:$PATH" haw commit-ai --format json # a haw.plugin.report/1
```

The first command prints a friendly one-liner. The second prints a JSON report you can
parse — something like:

```json
{
  "schema": "haw.plugin.report/1",
  "plugin": "commit-ai",
  "ok": true,
  "summary": "2 repo(s)",
  "findings": [
    { "level": "info", "message": "kernel: draft a commit" },
    { "level": "info", "message": "hal: draft a commit" }
  ]
}
```

![The `haw` command line — a plugin runs exactly like these built-in commands, as `haw <name>`](../assets/haw-cli.gif)

*No recompile, no core change: drop `haw-commit-ai` on your `PATH` and haw dispatches to it like any built-in.*

And because it also emits a `haw.plugin.view/1`, your plugin gets a home in the cockpit —
open `haw dash`, press `7`, and there it is in the Plugins panel:

![The `haw dash` cockpit, where plugins get their own Plugins view (press `7`)](../assets/haw-tui.gif)

## 9. Wire the MCP server into Claude Code

Now the Claude side. First install the MCP SDK — this is the only dependency, and only the
`--mcp` face needs it:

```bash
pip install mcp
```

Register the server with Claude Code — one command. Use an **absolute path** to your plugin
file:

```bash
claude mcp add haw-commit-ai -- python3 /abs/path/to/haw-commit-ai --mcp
```

Or, per project, drop it in a `.mcp.json` file at your project root:

```json
{
  "mcpServers": {
    "haw-commit-ai": {
      "command": "python3",
      "args": ["/abs/path/to/haw-commit-ai", "--mcp"]
    }
  }
}
```

Verify Claude sees the server:

```bash
claude mcp list            # haw-commit-ai should be listed
```

Inside a Claude session, `/mcp` shows the connected server and its tools. At Level 1 that's
`haw_context`, `repo_diff`, `changeset_repos_tool`, `write_commit`, and `draft_pr`. Once you
add Level 2 below, `changeset_diff` and `draft_changeset_pr` join them.

## 10. Worked example — Claude writes your commit and PR text

Make a change across two repos in your workspace (say `kernel` and `hal`), stage them, then
ask Claude — from inside the workspace directory:

> *"Read the diffs for the repos touched by my current changeset and write a
> conventional-commit message for each. Then draft a single cross-repo PR body. Commit each
> repo with its message; leave the PR as a dry draft."*

Claude will:

1. call **`changeset_repos_tool()`** → sees `kernel`, `hal`,
2. call **`repo_diff("kernel")`** and **`repo_diff("hal")`** → reads exactly what changed,
3. write conventional-commit messages (e.g. `fix(kernel): guard against null irq handler`),
4. call **`write_commit("kernel", …)`** and **`write_commit("hal", …)`** — each
   **path-guarded** to your workspace,
5. call **`draft_pr("kernel", "…", "…")`** → returns a PR body **dry** (nothing pushed).

You review the drafts, and when you're happy, run `haw change request` yourself (or let
Claude call `draft_pr(..., submit=True)`).

<div class="callout note">

**Being honest about Level 1.** At this point, Claude Code already sees a single repo's diff
natively — you haven't given it a superpower yet, you've just taught it to speak haw's
protocol cleanly. The real power arrives at **Level 2**: showing it an *entire* changeset,
spread across several repos, in a single view.

</div>

So far we've mostly reinvented what Claude does for one repo for free. Keep the faith: the
next level is the part it *can't* do on its own.

<!-- render via <img>; giphy hotlink — swap if giphy changes -->
<img class="meme" src="https://media.giphy.com/media/LBNbGeT9nwdEZdxNgj/giphy.gif" alt="This is fine — a cartoon dog sipping coffee as the room burns">

*"We built a whole plugin to do what Claude already did." This is fine — Level 2 fixes it.*

<div class="your-turn">

**Your turn (Level 1)**

- Scaffold your own: <code>haw plugins new commit-ai --lang python</code>, then run the zero-dependency face with <code>haw commit-ai --format json</code> and confirm you get a <code>haw.plugin.report/1</code> document.
- Drop the plugin on <code>PATH</code>, open <code>haw dash</code>, press <code>7</code>, and select <code>commit-ai</code> — your <code>haw.plugin.view/1</code> panel renders right in the cockpit.
- <code>pip install mcp</code>, register it with <code>claude mcp add …</code>, and ask Claude to read one repo's diff and propose a commit — <em>without</em> committing. Then let it call <code>write_commit</code>, and watch the path-guard in action by asking it to commit a path outside the workspace (it should refuse).

</div>

---

# Level 2 — the cross-repo power (changeset-wide)

Here's the pitch, sharp: **Claude alone can't see a fleet-wide changeset.** It can read one
repo's diff — but a haw changeset spans several repos at once (`kernel`, `hal`, `app`…), and
that combined story lives *between* the repos. A single-repo tool like lazygit can't show it
either. haw knows the whole changeset, so haw can hand Claude the whole picture.

We add **two tools to the same plugin** — no new script, no new server. They turn
`haw-commit-ai` from "a nice commit helper" into "the thing that gives an LLM fleet-wide
vision."

## 11. Why cross-repo is the unique value

Imagine you add one feature — say an `irq_mask` flag — and it has to land in three repos at
once: the `kernel` driver that owns the register, the `hal` layer that threads it through,
and the `app` that exposes it on the CLI. Each repo's diff, read alone, is a fragment. The
*meaning* — "these three moves are one feature and must land together" — only exists when
you see all three diffs side by side.

- Claude, on its own, reads one repo at a time. It can't see the fragments as one story.
- A single-repo tool sees one repo, full stop.
- **haw knows the changeset**, so it can concatenate every repo's diff into one document and
  hand that to Claude. Now Claude writes **one** coherent PR that narrates the whole feature.

That combined view is the unique value. Everything in Level 2 exists to deliver it.

## 12. Two cross-repo tools

Drop these alongside the Level 1 tools (the shipped
[`examples/plugins/haw-commit-ai/haw-commit-ai`](https://github.com/Nastwinns/hawser/tree/main/examples/plugins/haw-commit-ai)
already has them). First, the helper that builds the combined diff — it concatenates every
repo's diff under a clear `=== <repo> ===` header:

```python
def changeset_diff_text(ctx):
    """The COMBINED git diff across every repo of the current changeset."""
    repos = changeset_repos(ctx) or context_repos(ctx)
    if not repos:
        return "no changeset and no dirty repos — nothing to diff."
    chunks = []
    for r in repos:                                  # clear per-repo headers
        diff = repo_diff_text(r["path"]) if r.get("path") else ""
        body = diff.rstrip() if diff.strip() else "(no changes)"
        chunks.append(f"=== {r.get('name','?')} ===\n{body}")
    return "\n\n".join(chunks)                        # the whole story, top to bottom
```

This is the **killer function**: one call, and Claude sees `kernel`, `hal`, and `app`'s
diffs concatenated under `=== <repo> ===` headers — the fleet-wide changeset as a single
readable document.

Next, the helper that builds a single cross-repo PR skeleton — a combined-summary slot plus
one section per repo, which Claude then fills with prose:

```python
def draft_changeset_pr_body(ctx, title):
    """ONE coherent cross-repo PR skeleton narrating every repo together."""
    repos = changeset_repos(ctx) or context_repos(ctx)
    lines = [f"# {title}", "", "## Combined summary", "",
             "<!-- one narrative covering all repos and why they move together -->", "",
             "## Per-repo changes", ""]
    for r in repos:
        files, changed = diff_stat(r.get("path", "")) if r.get("path") else (0, 0)
        stat = f" ({files} file(s), {changed} line(s))" if files or changed else ""
        lines += [f"### {r.get('name','?')}{stat}", "", "<!-- what changed here and why -->", ""]
    lines += ["## Testing", "", "- [ ] `haw build`", "- [ ] `haw test`", ""]
    return "\n".join(lines)
```

Both are wrapped as MCP tools with the FastMCP decorator, exactly like the Level 1 ones —
add them inside `run_mcp()`:

```python
    @mcp.tool()
    def changeset_diff() -> str:
        """The COMBINED git diff across ALL changeset repos, with =-headers per repo.
        The fleet-wide view a single-repo tool can't give you."""
        return changeset_diff_text(read_context())

    @mcp.tool()
    def draft_changeset_pr(title: str) -> str:
        """ONE coherent cross-repo PR skeleton narrating every repo together.
        Dry — assembles the scaffold; you fill the prose, then `haw change request`."""
        return draft_changeset_pr_body(read_context(), title)
```

`draft_changeset_pr(title)` returns **one** PR body: the plugin assembles the skeleton
(per-repo sections plus a combined-summary slot), and **Claude fills the prose** from the
diffs. Dry by default; feed the result to `haw change request` to open the linked PRs across
the fleet.

<div class="callout note">

**`write_commit` stays path-guarded.** There's no cross-repo "write everything" tool by
design — for a changeset you commit **per repo** (Claude calls `write_commit` for each, each
guarded to `root`), then run `haw change request` to open the linked PRs across the fleet.
Writes stay small, reviewable, and inside your workspace.

</div>

## 13. Worked example — one PR for a three-repo changeset

Touch two or three repos in a changeset — say `kernel`, `hal`, and `app` — stage them, then
ask Claude, from inside the workspace:

> *"Call `changeset_diff()` to read my whole changeset, then `draft_changeset_pr()` and
> write one PR that tells the combined story — a section per repo plus a combined summary."*

Claude will:

1. call **`changeset_diff()`** → one document with `=== kernel ===`, `=== hal ===`,
   `=== app ===`, each repo's diff underneath,
2. call **`draft_changeset_pr("…")`** → gets the skeleton with a section per repo,
3. **fill the prose** into a single coherent narrative.

Here's an illustrative result (labelled as illustrative — your prose will match your actual
diffs):

```markdown
# feat: propagate the new irq-mask flag end to end

## Combined summary
A new `irq_mask` flag flows from the kernel driver up through the HAL and into
the app's config surface. The three repos move together so the feature lands atomically.

## Per-repo changes
### kernel
Add `irq_mask` to the driver's register write and guard the null-handler path.
### hal
Thread `irq_mask` through the HAL's `configure()` and expose it in the C header.
### app
Surface `--irq-mask` on the CLI and wire it to the HAL call.
```

**This is what neither Claude nor a single-repo tool can do alone.** A single-repo tool sees
three disconnected diffs; haw plus this plugin hand Claude the *changeset*, so it writes the
one story that spans them. When you're happy, commit each repo (`write_commit`, per repo) and
run `haw change request` to open the linked PRs across the fleet.

<!-- render via <img>; giphy hotlink — swap if giphy changes -->
<img class="meme" src="https://media.giphy.com/media/iyFmY2m2nfyAj5VFi8/giphy.gif" alt="Jubilant celebration reaction">

*One prompt. Three repos. One coherent PR narrative. That's the payoff — go celebrate.*

## 14. Safety notes — this is the important bit

Writing tools plus an LLM means guardrails matter. This is the section you *don't* skim. The
plugin bakes the guardrails in:

- **Path-guarded writes.** `write_commit` and `draft_pr(submit=True)` refuse any repo path
  that isn't inside the workspace `root` — Claude can't commit outside your fleet.
- **Dry by default.** `draft_pr` returns text only; it never pushes or force-pushes unless
  you explicitly pass `submit=True`.
- **No secrets in the plugin.** Forge authentication comes from *your* environment — haw's
  normal token resolution (`GITHUB_TOKEN`, and so on). The plugin stores nothing.
- **Separate process, honest exit codes.** The plugin runs out-of-process; a bug can't crash
  haw, and a non-zero exit propagates so CI still gates.

<div class="your-turn">

**Your turn (Level 2)**

- Touch <em>three</em> repos in a changeset, then ask Claude to call <code>changeset_diff()</code> and <code>draft_changeset_pr()</code> and write <strong>one</strong> PR narrative covering all three (kernel / hal / app plus a combined summary). Compare it to what you'd get by asking Claude repo-by-repo — the cross-repo story only appears when it sees the whole changeset at once.
- Extend the plugin: add a <code>repo_log(repo, n)</code> tool so Claude can see recent history for better messages. Keep it read-only.
- Add a per-repo line count to the combined diff output, so Claude knows which repo carries the bulk of the change before it starts writing.

</div>

---

## Glossary

- **plugin** — an executable named `haw-<name>` on your `PATH`; haw runs it as
  `haw <name>`.
- **PATH dispatch** — the convention (shared with git/cargo/kubectl) where haw runs an
  unknown subcommand by finding a matching executable on `PATH`.
- **`haw.plugin/1`** — the JSON context haw hands every plugin (via `HAW_JSON` or stdin),
  describing `root`, `stack`, and `repos`.
- **`haw.plugin.report/1`** — the machine report a plugin prints for `--format json`.
- **`haw.plugin.view/1`** — the cockpit panel a plugin prints when haw asks it to render
  (`HAW_RENDER=1`, `"intent": "render"`).
- **MCP (Model Context Protocol)** — a standard stdio JSON-RPC protocol that lets an AI
  assistant call tools you expose.
- **stdio** — a program's standard input and output streams; MCP messages travel over them.
- **tool** — a named function (with typed arguments) your MCP server advertises for Claude
  to call.
- **FastMCP** — the helper in the official MCP SDK that turns a Python function into an MCP
  tool with a decorator.

## What you learned

- A plugin is any executable named `haw-<name>` on `PATH`; haw hands it the
  **`haw.plugin/1`** context via `$HAW_JSON` / stdin and propagates its exit code.
- It can print a **`haw.plugin.report/1`** (`--format json`) and a **`haw.plugin.view/1`**
  panel (render intent, `HAW_RENDER=1`) for the cockpit's Plugins view (`7`).
- MCP is a standard stdio protocol that lets Claude call tools you expose; the same script
  becomes an **MCP server** with `--mcp`, using FastMCP.
- **Level 1 (single repo)** teaches the protocol: `haw_context`, `repo_diff`,
  `changeset_repos_tool`, `write_commit`, `draft_pr` — but Claude already sees one repo
  natively.
- **Level 2 (cross-repo)** is the real power: `changeset_diff` hands Claude the *combined*
  diff across the whole changeset, and `draft_changeset_pr` gets it to write **one**
  fleet-wide PR narrative — the cross-repo story neither Claude nor a single-repo tool can
  tell alone.
- **Guardrails:** path-guard writes to inside `root` (commit per repo, then
  `haw change request`), keep PR drafting dry by default, and never store secrets — auth
  stays in your environment.

## Where to next

You can now extend haw in any language and give an LLM safe, context-rich tools. From here:

- Browse the [Plugins reference](../PLUGINS.md) — lifecycle phases, the community index, and
  the language bindings.
- Study more real manifests in the [Examples index](../EXAMPLES.md).
- Keep the [CLI design and keymap](../CLI-DESIGN.md) handy.

That's the whole tool — now go build your own beam. Welcome aboard.
