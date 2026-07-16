# Security Policy

## Supported versions

`haw` is pre-1.0. Security fixes land on `main` and in the latest tagged release.

## Reporting a vulnerability

**Do not open a public issue for security problems.**

Report privately via GitHub's [private vulnerability reporting][advisories]
(Security → Report a vulnerability), or email **pros.balin@gmail.com**.

Please include:

- affected version / commit,
- a description and impact,
- reproduction steps or a proof of concept,
- any suggested remediation.

You can expect an acknowledgement within a few days. Once a fix is ready we will
coordinate disclosure and credit you (unless you prefer to stay anonymous).

## Scope

`haw` orchestrates `git` and the GitHub/GitLab APIs. Relevant areas include:
token handling (`HAW_*`/`GITHUB_TOKEN`/`GITLAB_TOKEN` are read from the
environment, never persisted), manifest/lockfile parsing, and shell-outs to
`git`. Vulnerabilities in those paths are in scope.

[advisories]: https://github.com/Nastwinns/keelson/security/advisories/new
