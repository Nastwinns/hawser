# hawser — release checklist

The exact steps to cut a hawser release and publish it. Grounded in the real
[`.github/workflows/release.yml`](../.github/workflows/release.yml), the workspace
[`Cargo.toml`](../Cargo.toml), and the templates under [`packaging/`](../packaging/).

Legend:
- 🔧 **automated** — happens in CI when you push the tag.
- 🔑 **needs a maintainer account/token** — you must do it, it can't be automated
  here (crates.io token, a new GitHub repo, Docker registry creds, Reddit/HN login).

---

## 1. Pre-flight (local, before tagging)

- [ ] On a clean checkout of `main`, working tree clean (`git status`).
- [ ] Green CI on the commit you're about to tag.
- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Version is correct. The workspace version is set once in
      [`Cargo.toml`](../Cargo.toml) under `[workspace.package] version` and each
      crate inherits it via `version.workspace = true`. Bump it there if this isn't
      `0.1.0` anymore. The path deps in `[workspace.dependencies]` also carry
      `version = "x"` — keep them in lockstep with the workspace version, or
      `cargo publish` will reject the graph.
- [ ] CHANGELOG note added for this version (create `CHANGELOG.md` if it doesn't
      exist yet — the GitHub Release itself auto-generates notes, but a hand-written
      changelog is worth keeping).
- [ ] README install section is accurate for what this release ships (see §7).

---

## 2. Tag & release (🔧 automated)

```bash
git tag v0.1.0
git push origin v0.1.0
```

Pushing a `v*` tag triggers [`release.yml`](../.github/workflows/release.yml). (You
can also run it manually via **workflow_dispatch** with the tag as input.)

**What the workflow produces**, for these **6 targets**:

| Target | OS | Archive |
|--------|----|---------|
| `x86_64-unknown-linux-gnu` | ubuntu | `tar.gz` |
| `x86_64-unknown-linux-musl` | ubuntu (musl-tools) | `tar.gz` |
| `aarch64-unknown-linux-gnu` | ubuntu (cross gcc) | `tar.gz` |
| `aarch64-apple-darwin` | macOS | `tar.gz` |
| `x86_64-apple-darwin` | macOS | `tar.gz` |
| `x86_64-pc-windows-msvc` | windows | `zip` |

For each: `cargo build --release -p hawser` (the `haw` binary), packaged as
`haw-<version>-<target>.{tar.gz,zip}` with a matching `.sha256`.

Then the `release` job:
1. Renders the packaging manifests with **real checksums** by running
   `packaging/render.py`, producing `dist/hawser.rb` (Homebrew) and
   `dist/hawser.json` (Scoop). Note: `render.py` is fed the sha256 for macOS
   arm64/x64, linux-gnu x64, and windows x64 — so the rendered Homebrew formula and
   Scoop manifest cover those platforms (musl and aarch64-linux ship as archives
   only).
2. **Signs** every archive with cosign (keyless, OIDC), emitting `.sig` + `.pem`
   next to each archive. This step is `continue-on-error: true`, so a cosign hiccup
   won't fail the release — verify the sigs are actually present (below).
3. Publishes a **GitHub Release** for the tag with auto-generated notes and uploads:
   `haw-*.tar.gz`, `haw-*.zip`, `haw-*.sha256`, `haw-*.sig`, `haw-*.pem`, plus the
   rendered `dist/hawser.rb` and `dist/hawser.json`.

**Verify the release:**

```bash
# Watch the run
gh run watch

# List the published assets (expect 6 archives + 6 .sha256 + sigs + rb/json)
gh release view v0.1.0

# Download and check a checksum
gh release download v0.1.0 -p 'haw-*-x86_64-unknown-linux-gnu.tar.gz*'
shasum -a 256 -c <(printf '%s  %s\n' "$(cat haw-*-linux-gnu.tar.gz.sha256)" haw-*-linux-gnu.tar.gz)

# Verify a cosign signature (keyless — identity is the release workflow's OIDC identity)
cosign verify-blob \
  --certificate haw-<ver>-x86_64-unknown-linux-gnu.tar.gz.pem \
  --signature   haw-<ver>-x86_64-unknown-linux-gnu.tar.gz.sig \
  --certificate-identity-regexp 'https://github.com/Nastwinns/hawser/.github/workflows/release.yml@.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  haw-<ver>-x86_64-unknown-linux-gnu.tar.gz

# Smoke-test the binary
tar xzf haw-*-x86_64-unknown-linux-gnu.tar.gz && ./haw --version
```

---

## 3. crates.io publish (🔑 needs crates.io token)

`cargo publish` requires a token: run `cargo login` once (get the token from
<https://crates.io/me>). Only a maintainer with publish rights on these crate names
can do this.

**Publish order** follows the workspace dependency graph
([`Cargo.toml`](../Cargo.toml)). Leaves first, `hawser` (the `haw` binary) last:

```
haw-core   → haw-git → haw-forge → haw-merge → haw-plugin → haw-tui → hawser
```

Rationale from the real dep edges: `haw-core`, `haw-merge`, and `haw-plugin` have no
intra-workspace deps (leaves); `haw-git`, `haw-forge`, and `haw-tui` each depend on
`haw-core`; `hawser` depends on core + git + forge + merge + tui. crates.io requires
every path dep to already be indexed, hence the order.

```bash
cargo publish -p haw-core
# wait for the crate to appear in the index before the next one
cargo publish -p haw-git
cargo publish -p haw-forge
cargo publish -p haw-merge
cargo publish -p haw-plugin
cargo publish -p haw-tui
cargo publish -p hawser        # this is the `haw` binary → `cargo install hawser`
```

**Wait-for-index note:** after each `cargo publish`, the crate takes a short moment
to land in the registry index. If the next publish fails with "no matching package
named …", wait ~30–60s and retry. A recent `cargo` mostly handles this, but don't
fire all seven back-to-back blindly.

**Plugin binaries** (`haw-artifact`, `haw-compliance`, `haw-git-gate`) discovered via
`haw <name>` on PATH: these can stay **git-only** for now, or be published **last**
(after `hawser`). `haw-compliance` depends on `haw-plugin`, so if you do publish it,
publish `haw-plugin` first (already in the order above). They're not required for
`cargo install hawser` to work.

**Dry-run first** to catch metadata/packaging problems without publishing:

```bash
cargo publish -p haw-core --dry-run
```

After `hawser` is indexed, confirm the headline install path from the README works:

```bash
cargo install hawser && haw --version
```

---

## 4. Homebrew tap (🔑 needs a new GitHub repo)

The release already rendered `dist/hawser.rb` with real checksums and attached it to
the GitHub Release. You just need a tap repo to host it.

```bash
# One-time: create the tap repo (must be named homebrew-tap under the org)
gh repo create Nastwinns/homebrew-tap --public \
  --description "Homebrew tap for hawser (haw)"

git clone https://github.com/Nastwinns/homebrew-tap
cd homebrew-tap
mkdir -p Formula

# Grab the rendered formula from the release and commit it
gh release download v0.1.0 -R Nastwinns/hawser -p hawser.rb -O Formula/hawser.rb
git add Formula/hawser.rb
git commit -m "hawser 0.1.0"
git push

# Test the install
brew install nastwinns/tap/hawser
haw --version

# On each new release, re-download the rendered hawser.rb and commit it.
```

---

## 5. Scoop bucket (🔑 needs a new GitHub repo — optional)

Same shape for Windows. The release rendered `dist/hawser.json`.

```bash
gh repo create Nastwinns/scoop-bucket --public \
  --description "Scoop bucket for hawser (haw)"

git clone https://github.com/Nastwinns/scoop-bucket
cd scoop-bucket
mkdir -p bucket
gh release download v0.1.0 -R Nastwinns/hawser -p hawser.json -O bucket/hawser.json
git add bucket/hawser.json
git commit -m "hawser 0.1.0"
git push

# Test (Windows / PowerShell)
scoop bucket add nastwinns https://github.com/Nastwinns/scoop-bucket
scoop install hawser
```

---

## 6. Docker image (🔑 needs registry creds)

Build from the [`Dockerfile`](../Dockerfile) at the repo root (multi-stage; the final
image keeps `git` because `haw` shells out to it for mutations).

```bash
# Build and smoke-test locally
docker build -t haw .
docker run --rm haw --version

# Tag for GHCR (adjust for Docker Hub if preferred)
docker tag haw ghcr.io/nastwinns/haw:0.1.0
docker tag haw ghcr.io/nastwinns/haw:latest

# Login (GHCR: a PAT with write:packages) and push
echo "$GHCR_TOKEN" | docker login ghcr.io -u <username> --password-stdin
docker push ghcr.io/nastwinns/haw:0.1.0
docker push ghcr.io/nastwinns/haw:latest
```

---

## 7. Post-release

- [ ] Update the README **Install** section if this release changed an install path
      (e.g. flip `cargo install hawser` from "soon" to live; add the `brew` line once
      the tap exists). Do this in a normal PR — not part of this checklist's file set.
- [ ] Sanity-check the docs site rebuilt: <https://nastwinns.github.io/hawser/docs/>
- [ ] Confirm the browser demo still loads: <https://nastwinns.github.io/hawser/>
- [ ] **Trigger the launch** (🔑 Reddit/HN accounts): only now, with v0.1.0 actually
      installable, work through [`LAUNCH-POSTS.md`](LAUNCH-POSTS.md) — post Show HN +
      Reddit the same day and engage the first 2 hours. The timing gate lives in
      [`LAUNCH.md §0`](LAUNCH.md).

---

## Maintainer-account summary

Steps that **cannot** be automated here and need your credentials:

| Step | What you need |
|------|---------------|
| crates.io publish (§3) | `cargo login` token with publish rights |
| Homebrew tap (§4) | Create `Nastwinns/homebrew-tap` |
| Scoop bucket (§5) | Create `Nastwinns/scoop-bucket` |
| Docker push (§6) | GHCR (or Docker Hub) login token |
| Launch posts (§7) | Reddit + Hacker News accounts |

Everything in §2 (build, archive, checksum, cosign sign, render rb/json, GitHub
Release) is automated by pushing the tag.
