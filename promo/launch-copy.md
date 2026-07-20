# hawser — launch & backlink copy

Ready-to-paste text for backlinks (awesome-lists) and launch posts.
Goal: drive discovery + durable backlinks so the site ranks on problem queries
(git submodules alternative, manage multiple git repos, reproducible multi-repo build).

---

## 1. Awesome-list entries

One-line entries. Each PR = an authority backlink + direct developer traffic.
Add under the most relevant category; follow each list's alphabetical / section rules.

### awesome-rust  (rust-unofficial/awesome-rust) — "Applications → Command-line utilities"
```
* [hawser](https://github.com/Nastwinns/hawser) — Reproducible multi-repo stacks + cross-repo PR/MR orchestration. One binary, one TUI.
```

### awesome-cli-apps  (agarrharr/awesome-cli-apps) — "Development"
```
- [hawser](https://github.com/Nastwinns/hawser) - Pin many git repos to one lockfile, then build, test and ship changes across the whole fleet.
```

### awesome-tuis  (rothgar/awesome-tuis) — "Development"
```
- [hawser](https://github.com/Nastwinns/hawser) - k9s-style cockpit to orchestrate reproducible multi-repo git stacks.
```

### awesome-devops / awesome-git  (various) — "Tools"
```
- [hawser](https://github.com/Nastwinns/hawser) - A git submodules alternative: reproducible multi-repo pinning + cross-forge change flow, in Rust.
```

PR checklist per list:
- Read CONTRIBUTING — many require the item to be non-trivial (stars/age) and alphabetized.
- Keep the description ≤ the list's char norm; match its dash style (`-` vs `*`).
- One list per PR.

---

## 2. Show HN

**Title** (HN strips "Show HN:" from the 80-char count; keep it tight):
```
Show HN: Hawser – pin your multi-repo stack to one lockfile (Rust)
```

**First comment** (post immediately as author — context, not marketing):
```
Hi HN. Your product often lives in 5-10 git repos and nobody remembers which commits
go together. Submodules give you detached HEADs and nested-checkout pain; a hand-rolled
clone script rots.

Hawser (`haw`) declares the repos in one manifest and writes a lockfile with exact SHAs,
so you, your CI and your teammate check out the identical tree every time. On top of the
composition it runs build/test in parallel across the fleet and opens cross-repo PRs/MRs
as a single changeset. One static binary, no Python, unsafe forbidden.

It is not a git wrapper — it orchestrates git + forge APIs, it doesn't reimplement merge.

Site + interactive course (try the TUI in the browser): https://nastwinns.github.io/hawser/
Repo: https://github.com/Nastwinns/hawser

Happy to answer questions on the lockfile format, the cockpit, or how it compares to
west / repo / meta / RepoFleet.
```

Timing: post Tue-Thu, ~08:00-10:00 ET. Reply fast to every comment in the first 2h.

---

## 3. r/rust

**Title**:
```
Hawser: reproducible multi-repo git stacks + cross-repo PR orchestration, in one Rust binary
```

**Body**:
```
I got tired of managing a product spread across many git repos — submodules detach HEADs,
clone scripts rot, and nobody agrees on which commits go together.

Hawser (`haw`) is a single static Rust binary that:

- declares the repos in one manifest and pins them to exact SHAs in a lockfile
  (reproducible checkout on every machine)
- runs `haw build` / `haw test` in parallel across the whole fleet
- opens cross-repo, cross-forge (GitHub/GitLab) pull/merge requests as one changeset
- ships a k9s-style TUI cockpit (bare `haw`)

`unsafe` is forbidden; no Python runtime. It orchestrates git + forge APIs rather than
reimplementing git.

Repo: https://github.com/Nastwinns/hawser
Site + browser demo: https://nastwinns.github.io/hawser/

Feedback on the manifest/lockfile design and the TUI welcome.
```

Also worth posting to: r/commandline, r/git, r/programming (link + short comment),
Lobsters (needs invite), and the This Week in Rust "call for participation" / project spotlight.

---

## Order of impact

1. awesome-lists PRs (durable backlinks, evergreen).
2. Show HN + r/rust same-day (traffic spike + more backlinks).
3. Reply to every thread — engagement drives both ranking signals and stars.
