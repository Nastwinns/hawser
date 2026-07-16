# quickstart — a workspace you can actually run

This example composes three **real, public** GitHub repos into two stacks, so you
can run the whole haw loop — sync, inspect, run commands across the fleet, and
start a cross-repo change — without any credentials.

- `octocat/Hello-World` @ `master` — shared *core* repo (in both stacks)
- `octocat/Spoon-Knife` @ `main` — the *web* front end
- `octocat/git-consortium` @ `master` — the *tooling* reference

```
site     = hello-world + spoon-knife
tooling  = hello-world + git-consortium
```

`hello-world` lives in **both** stacks — that shared-repo, one-fleet view is the
whole point of haw.

> All commands below take `--manifest examples/quickstart/haw.toml` when run from
> the repo root. To clone repos onto disk, copy this file into an empty directory
> first (see "Run it for real"), since `sync` writes `haw.lock` and checkouts next
> to the manifest.

## Inspect without cloning

`tree` reads the manifest only — no network, no checkout:

```console
$ haw tree --manifest examples/quickstart/haw.toml
examples/quickstart/haw.toml
├─ site
│  ├─ hello-world  master  (https://github.com/octocat/Hello-World.git)
│  └─ spoon-knife  main  (https://github.com/octocat/Spoon-Knife.git)
└─ tooling
   ├─ hello-world     master  (https://github.com/octocat/Hello-World.git)
   └─ git-consortium  master  (https://github.com/octocat/git-consortium.git)
```

## Run it for real

Copy the manifest into a fresh directory so checkouts land there:

```console
$ mkdir /tmp/haw-quickstart && cp examples/quickstart/haw.toml /tmp/haw-quickstart/
$ cd /tmp/haw-quickstart
```

### 1. Sync — clone every repo in a stack and pin `haw.lock`

haw needs a selected stack. Sync one directly with `--stack`:

```console
$ haw sync --stack site
wrote haw.lock (2 repos pinned)
  ✓ hello-world  cloned
  ✓ spoon-knife  cloned
synced stack `site` (2/2 repos)
```

Sync the second stack too — the shared `hello-world` is just updated, not
re-cloned:

```console
$ haw sync --stack tooling
  ✓ hello-world     updated
  ✓ git-consortium  cloned
synced stack `tooling` (2/2 repos)
```

`haw sync` wrote `haw.lock`, pinning each branch to the exact SHA it resolved —
commit that file for reproducible clones. (Tip: `haw switch <stack>` records a
stack as *current* so a bare `haw sync` / `haw status` targets it.)

### 2. Status — one dirty/drift line per repo

```console
$ haw status
REPO            BRANCH                   HEAD       DIRTY  DRIFT
hello-world     master                    7fd1a60b   -      -
spoon-knife     main                      d0dd1f61   -      -
git-consortium  (not cloned — run `haw sync`)
```

(The `not cloned` line appears for any repo in a stack you haven't synced yet.)

### 3. Run — a command in every repo, in parallel

```console
$ haw run 'git log -1 --oneline'
── hello-world ──
7fd1a60 Merge pull request #6 from Spaceghost/patch-1
── spoon-knife ──
d0dd1f6 Pointing to the guide for forking
ran in 2/2 repos
```

### 4. Change — one branch across many repos

Start a coordinated feature branch across the repos it touches:

```console
$ haw change start DEMO --repos hello-world,spoon-knife
changeset `DEMO` started across 2 repo(s):
  hello-world  -> change/DEMO
  spoon-knife  -> change/DEMO
```

Every affected repo is now on `change/DEMO`:

```console
$ haw status
REPO            BRANCH                   HEAD       DIRTY  DRIFT
hello-world     change/DEMO               7fd1a60b   -      -
spoon-knife     change/DEMO               d0dd1f61   -      -
git-consortium  (not cloned — run `haw sync`)

$ haw change list
DEMO
```

From here you'd commit in each repo, then `haw change request DEMO` to open
cross-linked PRs and `haw change land DEMO` to merge them in dependency order.

## Next

- The reading-only themed manifests: [`../automotive-pinned`](../automotive-pinned/),
  [`../embedded-bsp`](../embedded-bsp/), [`../governance`](../governance/).
- The plugin example: [`../haw-hello`](../haw-hello/).
- Back to the [examples index](../README.md).
