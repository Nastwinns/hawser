# Distributing hawser to private registries

Every tagged release publishes a signed [GitHub Release](https://github.com/Nastwinns/hawser/releases/latest)
first. After that succeeds, the `distribute` job in
[`.github/workflows/release.yml`](https://github.com/Nastwinns/hawser/blob/main/.github/workflows/release.yml) mirrors the **same
artifacts** to any of four private registries — **Nexus**, **Artifactory**, **GitLab**,
and **Bitbucket** — for organizations that install from an internal mirror rather than
from GitHub.

Each registry is **opt-in**: a target is only attempted when its secrets are configured.
A repo with no secrets set still releases successfully — the `distribute` job logs a
clear `skipping <target>: secret not set` and the whole job is `continue-on-error`, so a
registry outage can never fail the release.

## What gets uploaded

For every configured registry, the job uploads the full artifact set for the release:

- `haw-<version>-<target>.tar.gz` / `.zip` — the platform archives
- `haw-<version>-<target>.<ext>.sha256` — SHA-256 checksums
- `haw-<version>-<target>.<ext>.sig` and `.pem` — cosign keyless signature + certificate
- `hawser_<version>-1_amd64.deb` and `hawser-<version>-1.x86_64.rpm` — Linux packages

These are the identical files attached to the GitHub Release, so checksums and cosign
signatures verify the same way regardless of which mirror you pulled from (see
[INSTALL.md → Verify](INSTALL.md#verify-the-cosign-signature)).

## Secret matrix

Configure these as GitHub Actions repository (or organization) secrets. Only the
registries whose required secrets are present will be published to.

| Registry | Secret | Required? | Default | Purpose |
|----------|--------|-----------|---------|---------|
| **Nexus** | `NEXUS_URL` | required | — | Base URL, e.g. `https://nexus.example.com` |
| | `NEXUS_USER` | required | — | Username |
| | `NEXUS_PASS` | required | — | Password / token |
| | `NEXUS_REPO` | optional | `raw-hosted` | Raw hosted repo name |
| **Artifactory** | `ARTIFACTORY_URL` | required | — | Base URL, e.g. `https://artifactory.example.com/artifactory` |
| | `ARTIFACTORY_TOKEN` | required | — | Bearer / identity token |
| | `ARTIFACTORY_REPO` | optional | `generic-local` | Generic repo key |
| **GitLab** | `GITLAB_TOKEN` | required | — | Personal/project access token (`api` scope) |
| | `GITLAB_PROJECT_ID` | required | — | Numeric project ID |
| | `GITLAB_URL` | optional | `https://gitlab.com` | Self-managed instance base URL |
| **Bitbucket** | `BITBUCKET_USER` | required | — | Username |
| | `BITBUCKET_TOKEN` | required | — | App password / access token |
| | `BITBUCKET_WORKSPACE` | required | — | Workspace slug |
| | `BITBUCKET_REPO` | required | — | Repository slug |

A registry is skipped (logged, not failed) unless **all** its *required* secrets are set.

## Per-registry layout and install

Throughout, `<version>` is the tag without the leading `v` (e.g. `0.1.7`).

### Nexus (raw hosted repository)

Each file is `PUT` to a raw hosted repo under a versioned path:

```
<NEXUS_URL>/repository/<NEXUS_REPO>/haw/<version>/<file>
```

Upload (what CI runs, per file):

```bash
curl -u "$NEXUS_USER:$NEXUS_PASS" \
  --upload-file haw-0.1.7-x86_64-unknown-linux-musl.tar.gz \
  "$NEXUS_URL/repository/raw-hosted/haw/0.1.7/haw-0.1.7-x86_64-unknown-linux-musl.tar.gz"
```

Consume:

```bash
curl -u "$NEXUS_USER:$NEXUS_PASS" -O \
  "$NEXUS_URL/repository/raw-hosted/haw/0.1.7/haw-0.1.7-x86_64-unknown-linux-musl.tar.gz"
tar xzf haw-0.1.7-x86_64-unknown-linux-musl.tar.gz && sudo install haw /usr/local/bin/
```

### Artifactory (generic repository)

Each file is `PUT` (Bearer auth) to a generic repo under a versioned path:

```
<ARTIFACTORY_URL>/<ARTIFACTORY_REPO>/haw/<version>/<file>
```

Upload (per file):

```bash
curl -H "Authorization: Bearer $ARTIFACTORY_TOKEN" \
  --upload-file haw-0.1.7-x86_64-unknown-linux-musl.tar.gz \
  "$ARTIFACTORY_URL/generic-local/haw/0.1.7/haw-0.1.7-x86_64-unknown-linux-musl.tar.gz"
```

Consume:

```bash
curl -H "Authorization: Bearer $ARTIFACTORY_TOKEN" -O \
  "$ARTIFACTORY_URL/generic-local/haw/0.1.7/haw-0.1.7-x86_64-unknown-linux-musl.tar.gz"
```

### GitLab (generic package registry + Release)

Two things happen. First, each file is `PUT` to the project's **generic package
registry**:

```
<GITLAB_URL>/api/v4/projects/<GITLAB_PROJECT_ID>/packages/generic/haw/<version>/<file>
```

Then a **GitLab Release** is created for the tag, with `assets.links[]` pointing at each
uploaded package file (an existing release for the tag is tolerated, not an error).

Upload (per file):

```bash
curl --header "PRIVATE-TOKEN: $GITLAB_TOKEN" \
  --upload-file haw-0.1.7-x86_64-unknown-linux-musl.tar.gz \
  "https://gitlab.com/api/v4/projects/$GITLAB_PROJECT_ID/packages/generic/haw/0.1.7/haw-0.1.7-x86_64-unknown-linux-musl.tar.gz"
```

Consume:

```bash
curl --header "PRIVATE-TOKEN: $GITLAB_TOKEN" -O \
  "https://gitlab.com/api/v4/projects/$GITLAB_PROJECT_ID/packages/generic/haw/0.1.7/haw-0.1.7-x86_64-unknown-linux-musl.tar.gz"
```

Or open the project's **Deploy → Releases** page and download from the release assets.

### Bitbucket (repository Downloads)

Each file is `POST`ed (multipart) to the repository's **Downloads** area:

```
https://api.bitbucket.org/2.0/repositories/<BITBUCKET_WORKSPACE>/<BITBUCKET_REPO>/downloads
```

Upload (per file):

```bash
curl -u "$BITBUCKET_USER:$BITBUCKET_TOKEN" \
  -X POST \
  "https://api.bitbucket.org/2.0/repositories/$BITBUCKET_WORKSPACE/$BITBUCKET_REPO/downloads" \
  -F files=@haw-0.1.7-x86_64-unknown-linux-musl.tar.gz
```

Consume (files land under the repo's Downloads tab; filenames are flat, not versioned):

```bash
curl -u "$BITBUCKET_USER:$BITBUCKET_TOKEN" -O -L \
  "https://bitbucket.org/$BITBUCKET_WORKSPACE/$BITBUCKET_REPO/downloads/haw-0.1.7-x86_64-unknown-linux-musl.tar.gz"
```

> Bitbucket Downloads is a flat namespace (no per-version folders), so the `<version>`
> is carried in the filename itself.

## `haw publish` — upload artifacts from the CLI

The CI `distribute` job mirrors *release* archives, but you can push **any** artifacts
(build outputs, an `evidence` bundle, an SBOM) to the same four registries yourself with
`haw publish`. It uses the identical upload paths and auth as CI, reading credentials from
the same environment variables.

```
haw publish <files…> --to <nexus|artifactory|gitlab|bitbucket>
            [--name <NAME>] [--version <VER>] [--url <URL>]
            [--dry-run] [--insecure] [--format json]
```

| Flag | Meaning |
|------|---------|
| `<files…>` | Files or globs to upload. Defaults to `haw-evidence.tar.gz` if present and no files are given. |
| `--to` | Target registry: `nexus`, `artifactory`, `gitlab`, or `bitbucket` (required). |
| `--name` | Package name. Default: the current stack, else the workspace directory name. |
| `--version` | Package version. Default: the short HEAD SHA, else `unversioned`. |
| `--url` | Override the target's base URL (else taken from the target's env var). |
| `--dry-run` | Print exactly what would upload (method, URL, auth slot) and exit — no network, no credentials needed. |
| `--insecure` | Allow a non-HTTPS (`http://`) registry. **By default `http://` registries are rejected**; without this flag `haw publish` refuses to send credentials in cleartext. |
| `--format json` | Emit a JSON summary `{target, name, version, uploads:[…]}`. |

Credentials come from the environment, per target (same variables as the CI secret
matrix above):

| Target | Env vars |
|--------|----------|
| **Nexus** | `NEXUS_URL`, `NEXUS_USER`, `NEXUS_PASS`, optional `NEXUS_REPO` (default `raw-hosted`) |
| **Artifactory** | `ARTIFACTORY_URL`, `ARTIFACTORY_TOKEN`, optional `ARTIFACTORY_REPO` (default `generic-local`) |
| **GitLab** | `GITLAB_TOKEN`, `GITLAB_PROJECT_ID`, optional `GITLAB_URL` (default `https://gitlab.com`) |
| **Bitbucket** | `BITBUCKET_USER`, `BITBUCKET_TOKEN`, `BITBUCKET_WORKSPACE`, `BITBUCKET_REPO` |

```bash
haw publish ./out/*.bin --to nexus                 # upload build outputs to Nexus raw-hosted
haw publish --to gitlab                             # upload haw-evidence.tar.gz to GitLab packages
haw publish sbom.json haw-evidence.tar.gz --to artifactory   # several files at once
haw publish app.bin --to bitbucket                  # POST to the Bitbucket repo Downloads
haw publish app.bin --to nexus --dry-run            # print the plan (method/URL/auth), no network
haw publish app.bin --to nexus --format json        # machine-readable upload summary
```

## Verifying after download

Regardless of the mirror, verify exactly as with the GitHub Release — download the
matching `.sha256`, `.sig`, and `.pem` alongside the archive:

```bash
sha256sum -c haw-0.1.7-x86_64-unknown-linux-musl.tar.gz.sha256
cosign verify-blob \
  --certificate haw-0.1.7-x86_64-unknown-linux-musl.tar.gz.pem \
  --signature   haw-0.1.7-x86_64-unknown-linux-musl.tar.gz.sig \
  --certificate-identity-regexp 'https://github.com/Nastwinns/hawser' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  haw-0.1.7-x86_64-unknown-linux-musl.tar.gz
```

See [INSTALL.md](INSTALL.md#prebuilt-archives-signed) for the full verification and
air-gap workflow.

---

Back to [INSTALL.md](INSTALL.md).
