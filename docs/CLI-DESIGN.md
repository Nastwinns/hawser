# CLI design — lexicon & options

Goal: a lexicon a new user understands without a glossary, and options that fix what
`repo`/`west` users always missed.

## Lexicon (canonical since v0.1)

| Term | Meaning | Replaces / rejected |
|------|---------|---------------------|
| **repo** | one Git repository in the workspace (`[repo.NAME]`) | ~~brick~~ (accepted alias), `project` (repo-tool jargon) |
| **stack** | a named composition of repos (`[stack.NAME]`, `repos = [...]`) | ~~product~~ (accepted alias) |
| **overlay** | named per-repo overrides applied at lock time | `profile`, `variant` |
| **changeset** | one feature across N repos (branch + PR/MRs) | `topic`, `issue` |
| **group** | free-form label on a repo, used to filter commands | kept from `repo` tool, now actually wired |
| **rev** | what you ask for: branch, tag, or SHA — kind auto-detected | `revision`, `refspec` |
| **lock / pin** | resolved SHA in `keel.lock` | `freeze` (planned rename: `keel pin` / `keel unpin`) |
| **drift** | HEAD differs from the locked SHA | — |

Old spellings (`brick`, `product`, `bricks`, `--product`, `--bricks`) parse forever as
aliases; serialization and docs use the new words only.

## Rev handling (user-friendly by default)

- One field: `rev = "main" | "v6.1.2" | "<40-hex sha>"`. No `type =` key; the kind is
  detected (`refs/heads` > peeled tag > tag > full SHA).
- Display: SHAs are shown 8 chars everywhere; `keel.lock` stores the full 40.
- Never detached: branch revs check out on a same-name branch, tags/SHAs on `keel/<rev>`.

## Groups (implemented)

- `groups = ["firmware", "ci"]` on a repo.
- `keel sync --group firmware`, `keel status --group ci`, `keel forall --group firmware -c ...`
  (repeatable; empty filter = everything; a filter excludes ungrouped repos).
- Groups are recorded in `keel.lock` so filtering works offline.

## Options grid

| Option | Commands | Note |
|--------|----------|------|
| `--stack <S>` | sync, graph | alias `--product`; default: last `switch`, else the only stack |
| `--overlay <O>` | lock, sync*, graph | repeatable, later wins; *sync only when generating the lock |
| `--group <G>` | sync, status, forall | repeatable |
| `--repos a,b` | change start | alias `--bricks` |
| `-j, --jobs <N>` | sync, switch, forall | default min(cores, 8) |
| `--skip-branch` | change start | adopt current branches (RepoFleet) |
| `--branch <B>` | change start | default `change/<id>` |

## Planned (not yet implemented)

- `keel pin` / `keel unpin` — friendlier `freeze`/`unfreeze` (Phase 2).
- `--label <L>` on `change start` — forwarded to PR/MRs at `change request` (Phase 3).
- `forge = "github" | "gitlab"` key on `[remote.X]` for hosts the URL heuristic misses.
- `deps = [...]` on a repo — required before `change land` can claim topological order.
- Tag conveniences: `keel lock --as-of <tag>`; `keel status` marking `rev` kind (branch/tag/sha).
