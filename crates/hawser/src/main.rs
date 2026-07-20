mod commands;
mod controller;
mod publish;
mod ui;

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
#[cfg(test)]
use haw_core::change;
use haw_core::git::GitBackend;
use haw_core::manifest::{ManifestLoader, TomlLoader, edit, import};
use haw_core::plugin::{self, Dispatch, ProcessRunner, RepoContext};
use haw_core::workspace::{CloneTuning, MANIFEST_FILE, RepoStatus, Workspace};
use haw_core::{audit, hooks, resolver};
use haw_forge::Tokens;
#[cfg(test)]
use haw_forge::orchestrate;
use haw_git::ShellGit;
use haw_git::parallel::fan_out;
use serde_json::json;

use commands::change::{
    change_goto, change_land, change_list, change_request, change_start, change_status,
    snapshot_list, snapshot_restore, snapshot_save,
};
#[cfg(test)]
use commands::change::{
    change_land_value, change_request_value, change_start_value, change_status_value,
};
use commands::merge::{merge_abort, merge_cleanup, merge_plan, merge_resolve, merge_status};
use commands::publish::publish_cmd;
use commands::run::build_or_test;
#[cfg(test)]
use commands::run::build_test_value;
use commands::sync::sync;
use controller::cli::CliController;
use controller::demo::DemoController;
use ui::palette::Palette;

#[derive(Parser)]
#[command(
    name = "haw",
    version,
    about = "The beam that binds the repos",
    after_help = "\
Examples:
  $ haw init haw.toml           bootstrap a workspace from a manifest
  $ haw sync                     clone/update every repo, writing haw.lock
  $ haw tree                     print the stack -> repo composition
  $ haw status                   dirty/drift/ahead-behind per repo
  $ haw change start FEAT-42     branch across every affected repo
  $ haw                          open the fleet cockpit (bare, no subcommand)

Run `haw <command> --help` for that command's own examples."
)]
struct Cli {
    /// Path to the manifest.
    #[arg(long, global = true, default_value = "haw.toml")]
    manifest: PathBuf,

    /// No subcommand opens the TUI cockpit (same as `haw dash`).
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Bootstrap a workspace from a manifest file or URL.
    #[command(after_help = "\
Examples:
  $ haw init haw.toml                                 from a local file
  $ haw init https://example.com/fleet/haw.toml       from a URL
  $ haw --manifest custom.toml init haw.toml           bootstrap under a custom filename")]
    Init {
        /// Path or http(s) URL of an existing haw.toml.
        source: String,
        /// Allow insecure (http://) or internal-IP URLs when fetching.
        #[arg(long)]
        insecure: bool,
    },
    /// Clone/update repos to the state in haw.lock (writes it if absent).
    #[command(after_help = "\
Examples:
  $ haw sync                          clone/update every repo in the current stack
  $ haw sync --stack gateway          sync one specific stack
  $ haw sync --locked                 CI gate: fail unless haw.lock already exists
  $ haw sync --shared                 clone via a local mirror cache (git alternates)
  $ haw sync --filter blob:none       partial clone: keep all commits, lazy blobs (scales to 1000s of repos)
  $ haw sync --depth 1                shallow clone: truncated history (smaller, may deepen for old pins)
  $ haw sync --recurse-submodules     init/update each repo's git submodules (pinned to the superproject)
  $ haw sync --group firmware -j 4    only `firmware`-grouped repos, 4 parallel jobs")]
    Sync {
        /// CI contract: fail unless haw.lock exists (no rev resolution).
        #[arg(long)]
        locked: bool,
        #[arg(long = "stack", alias = "product")]
        stack: Option<String>,
        /// Overlays only apply when the lock is generated.
        #[arg(long)]
        overlay: Vec<String>,
        /// Only repos in these groups (repeatable).
        #[arg(long = "group")]
        groups: Vec<String>,
        /// Share objects with a local mirror cache (git alternates, no symlinks).
        #[arg(long)]
        shared: bool,
        /// Partial clone: `git clone --filter=<spec>` (e.g. blob:none, tree:0).
        /// Keeps ALL commits so any locked SHA stays reachable; blobs fetch
        /// lazily. The reproducibility-safe lever for pinned revs. Overrides
        /// `[defaults] filter` in haw.toml.
        #[arg(long, value_name = "SPEC")]
        filter: Option<String>,
        /// Shallow clone: `git clone --depth <N>`. Faster/smaller, but the
        /// locked SHA may not be in the truncated history — haw will deepen to
        /// reach an old locked SHA; --filter=blob:none is safer for pinned
        /// revs. Overrides `[defaults] depth` in haw.toml.
        #[arg(long, value_name = "N")]
        depth: Option<u32>,
        /// Recurse git submodules for every repo this run: pass
        /// `--recurse-submodules` at clone time and run `git submodule update
        /// --init --recursive` on existing clones. Overrides the manifest's
        /// per-repo `submodules` and `[defaults] submodules`. Submodules follow
        /// the superproject's pinned commit, so this stays reproducible.
        #[arg(long)]
        recurse_submodules: bool,
        #[arg(long, short = 'j')]
        jobs: Option<usize>,
    },
    /// Resolve every repo's rev to a SHA and (re)write haw.lock.
    #[command(after_help = "\
Examples:
  $ haw lock                    resolve every repo's manifest rev -> haw.lock
  $ haw lock --overlay dev       resolve using the `dev` overlay's rev overrides
  $ haw lock --format json       machine-readable resolved revs (schema haw.lock/1)")]
    Lock {
        #[arg(long)]
        overlay: Vec<String>,
        /// `text` (default) or `json` (schema haw.lock/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Pin haw.lock to each repo's current HEAD (no network).
    #[command(
        alias = "freeze",
        after_help = "\
Examples:
  $ haw pin       snapshot every repo's current checkout into haw.lock (no network)"
    )]
    Pin,
    /// Restore haw.lock to the manifest revs (same as `haw lock`).
    #[command(
        alias = "unfreeze",
        after_help = "\
Examples:
  $ haw unpin                    restore haw.lock to the manifest's declared revs
  $ haw unpin --overlay dev       ...using the `dev` overlay's rev overrides"
    )]
    Unpin {
        #[arg(long)]
        overlay: Vec<String>,
    },
    /// Add, remove, or list the repos of the manifest.
    #[command(alias = "brick")]
    Repo {
        #[command(subcommand)]
        command: RepoCommand,
    },
    /// Add, remove, or list the stacks of the manifest.
    #[command(alias = "product")]
    Stack {
        #[command(subcommand)]
        command: StackCommand,
    },
    /// Aggregated fleet status: branch, head, dirty, drift per repo.
    #[command(
        alias = "st",
        after_help = "\
Examples:
  $ haw status                       branch/head/dirty/drift for every repo
  $ haw status --group firmware       only `firmware`-grouped repos
  $ haw status --format json          machine-readable (schema haw.status/1)
  $ haw status --verify               exit 3 if anything is missing, dirty, or drifted (CI gate)"
    )]
    Status {
        /// Only repos in these groups (repeatable).
        #[arg(long = "group")]
        groups: Vec<String>,
        /// `text` (default) or `json` (schema haw.status/1).
        #[arg(long, default_value = "text")]
        format: String,
        /// Exit 3 when any repo is missing, dirty, or drifted (CI gate).
        #[arg(long)]
        verify: bool,
    },
    /// Record a stack as current and sync it.
    #[command(after_help = "\
Examples:
  $ haw switch sensor-node       record `sensor-node` as current, then sync it
  $ haw switch gateway -j 8       ...with 8 parallel sync jobs
  $ haw switch gateway --filter blob:none   partial-clone the stack (keeps all commits)
  $ haw switch gateway --depth 1            shallow-clone the stack (may deepen for old pins)")]
    Switch {
        stack: String,
        /// Partial clone: `git clone --filter=<spec>` (e.g. blob:none, tree:0).
        /// Keeps ALL commits so any locked SHA stays reachable; blobs fetch
        /// lazily. Overrides `[defaults] filter` in haw.toml.
        #[arg(long, value_name = "SPEC")]
        filter: Option<String>,
        /// Shallow clone: `git clone --depth <N>`. Smaller, but may need to
        /// deepen to reach an old locked SHA; --filter=blob:none is safer for
        /// pinned revs. Overrides `[defaults] depth` in haw.toml.
        #[arg(long, value_name = "N")]
        depth: Option<u32>,
        #[arg(long, short = 'j')]
        jobs: Option<usize>,
    },
    /// Print the stack -> repo tree.
    #[command(
        alias = "graph",
        after_help = "\
Examples:
  $ haw tree                       every stack -> repo composition
  $ haw tree --stack gateway        just one stack
  $ haw tree --overlay dev          composition after applying the `dev` overlay
  $ haw tree --format json          machine-readable (schema haw.tree/1)"
    )]
    Tree {
        #[arg(long = "stack", alias = "product")]
        stack: Option<String>,
        #[arg(long)]
        overlay: Vec<String>,
        /// `text` (default) or `json` (schema haw.tree/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Run a command in every repo, in parallel.
    #[command(
        alias = "forall",
        after_help = "\
Examples:
  $ haw run 'git fetch --tags'          quote multi-word commands
  $ haw run -c 'git status -s'           repo-tool-style -c flag also works
  $ haw run --group firmware 'make'       only `firmware`-grouped repos
  $ haw run 'make' --format json          machine-readable (schema haw.run/1)"
    )]
    Run {
        /// The command (positional; `-c` also works, repo-tool style).
        #[arg(required_unless_present = "command_flag")]
        command: Option<String>,
        #[arg(short = 'c', long = "command", conflicts_with = "command")]
        command_flag: Option<String>,
        /// Only repos in these groups (repeatable).
        #[arg(long = "group")]
        groups: Vec<String>,
        #[arg(long, short = 'j')]
        jobs: Option<usize>,
        /// `text` (default) or `json` (schema haw.run/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Grep across every cloned repo (or one stack) with `git grep`.
    #[command(after_help = "\
Examples:
  $ haw grep TODO                    search every cloned repo for TODO
  $ haw grep 'fn main' --stack gateway   only the `gateway` stack's repos
  $ haw grep panic --json             machine-readable (array of {repo,path,line,text})")]
    Grep {
        /// The pattern passed to `git grep -e`.
        pattern: String,
        /// Limit to one stack's repos (default: the whole fleet).
        #[arg(long = "stack", alias = "product")]
        stack: Option<String>,
        /// Emit JSON instead of grouped text.
        #[arg(long)]
        json: bool,
    },
    /// Cross-repo feature (changeset) workflow.
    #[command(after_help = "\
Examples:
  $ haw change start FEAT-42 --repos kernel,hal    branch across two repos
  $ haw change status FEAT-42                       per-repo branch + PR/CI dashboard
  $ haw change request FEAT-42                       open cross-linked PR/MRs
  $ haw change land FEAT-42                          merge them in dependency order

Run `haw change <subcommand> --help` for that subcommand's own examples.")]
    Change {
        #[command(subcommand)]
        command: ChangeCommand,
    },
    /// Assert the on-disk tree matches haw.lock; exit 3 on drift (CI gate).
    #[command(after_help = "\
Examples:
  $ haw verify                    exit 3 if any repo drifted from haw.lock (CI gate)
  $ haw verify --format json       machine-readable (schema haw.status/1)")]
    Verify {
        /// `text` (default) or `json` (schema haw.status/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Run each repo's `build` command from the manifest, in parallel.
    #[command(after_help = "\
Examples:
  $ haw build                       run every repo's declared `build =` command
  $ haw build --group firmware       only `firmware`-grouped repos
  $ haw build --format json          machine-readable (schema haw.build/1)")]
    Build {
        /// Only repos in these groups (repeatable).
        #[arg(long = "group")]
        groups: Vec<String>,
        #[arg(long, short = 'j')]
        jobs: Option<usize>,
        /// `text` (default) or `json` (schema haw.build/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Run each repo's `test` command from the manifest, in parallel.
    #[command(after_help = "\
Examples:
  $ haw test                       run every repo's declared `test =` command
  $ haw test --group firmware -j 2  only `firmware`-grouped repos, 2 parallel jobs
  $ haw test --format json          machine-readable (schema haw.test/1)")]
    Test {
        /// Only repos in these groups (repeatable).
        #[arg(long = "group")]
        groups: Vec<String>,
        #[arg(long, short = 'j')]
        jobs: Option<usize>,
        /// `text` (default) or `json` (schema haw.test/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Manage lifecycle hooks (.haw/hooks) and git integrity hooks.
    #[command(after_help = "\
Examples:
  $ haw hooks install       write a pre-commit hook (runs `haw verify`) in every repo
  $ haw hooks list          show the lifecycle hooks this workspace defines")]
    Hooks {
        #[command(subcommand)]
        command: HooksCommand,
    },
    /// Bundle baseline evidence (manifest, lock, audit log, status) for audits.
    #[command(after_help = "\
Examples:
  $ haw evidence                        bundle into ./haw-evidence.tar.gz
  $ haw evidence --out release.tar.gz    choose the output archive path")]
    Evidence {
        /// Output archive path.
        #[arg(long, default_value = "haw-evidence.tar.gz")]
        out: PathBuf,
    },
    /// Upload FLEET artifacts to a generic/raw artifact registry.
    #[command(after_help = "\
Examples:
  $ haw publish ./out/*.bin --to nexus            upload build outputs to Nexus raw-hosted
  $ haw publish --to gitlab                        upload haw-evidence.tar.gz to GitLab generic packages
  $ haw publish sbom.json haw-evidence.tar.gz --to artifactory   several files at once
  $ haw publish app.bin --to bitbucket             POST to Bitbucket repo Downloads
  $ haw publish app.bin --to nexus --dry-run       print the plan (method/URL/auth), no network
  $ haw publish app.bin --to nexus --format json   machine-readable upload summary

Credentials come from the environment (the CI matrix). Missing creds without
--dry-run is a clear error; --dry-run never touches the network.
  nexus:       NEXUS_URL NEXUS_USER NEXUS_PASS [NEXUS_REPO=raw-hosted]
  artifactory: ARTIFACTORY_URL ARTIFACTORY_TOKEN [ARTIFACTORY_REPO=generic-local]
  gitlab:      [GITLAB_URL=https://gitlab.com] GITLAB_TOKEN GITLAB_PROJECT_ID
  bitbucket:   BITBUCKET_USER BITBUCKET_TOKEN BITBUCKET_WORKSPACE BITBUCKET_REPO")]
    Publish {
        /// Files/globs to upload (default: haw-evidence.tar.gz if present).
        files: Vec<String>,
        /// Registry to upload to.
        #[arg(long, value_name = "nexus|artifactory|gitlab|bitbucket")]
        to: String,
        /// Package name (default: current stack, else the workspace directory).
        #[arg(long)]
        name: Option<String>,
        /// Package version (default: short HEAD SHA, else `unversioned`).
        #[arg(long)]
        version: Option<String>,
        /// Override the target's base URL (else from the target's env var).
        #[arg(long)]
        url: Option<String>,
        /// Print exactly what would upload and exit; no network, no creds needed.
        #[arg(long)]
        dry_run: bool,
        /// Allow a non-https (`http://`) registry — credentials are sent in cleartext.
        #[arg(long)]
        insecure: bool,
        /// Emit a JSON summary {target, name, version, uploads:[...]}.
        #[arg(long)]
        format: Option<String>,
    },
    /// Convert a west.yml or repo default.xml manifest to haw.toml.
    #[command(after_help = "\
Examples:
  $ haw import --from west.yml            convert a west manifest
  $ haw import --from default.xml          convert a Google `repo` manifest
  $ haw import --from west.yml --manifest new.toml   write the result to a custom filename")]
    Import {
        /// Path to the foreign manifest.
        #[arg(long)]
        from: PathBuf,
    },
    /// Parallel collaborative merge: slice one big merge into reviewable units.
    #[command(after_help = "\
Examples:
  $ haw merge plan origin/feature --repo kernel     slice a merge into per-directory units
  $ haw merge resolve src --take theirs --repo kernel   auto-resolve one slice
  $ haw merge status --repo kernel                   show slices and resolution state
  $ haw merge cleanup --repo kernel -m 'merge feature'  seal it as one merge commit
  $ haw merge abort --repo kernel                    give up and restore the target branch

Run `haw merge <subcommand> --help` for that subcommand's own examples.")]
    Merge {
        #[command(subcommand)]
        command: MergeCommand,
    },
    /// Print a shell completion script to stdout.
    #[command(after_help = "\
Examples:
  $ haw completions zsh > ~/.zfunc/_haw     install zsh completions
  $ haw completions bash > /etc/bash_completion.d/haw
  $ haw completions fish > ~/.config/fish/completions/haw.fish

Supported shells: bash, zsh, fish, powershell, elvish.")]
    Completions {
        /// Shell to generate completions for.
        shell: clap_complete::Shell,
    },
    /// Open a repo's forge page in the browser (mirrors the TUI `o`).
    #[command(after_help = "\
Examples:
  $ haw open kernel       open kernel's forge homepage in the browser
  $ haw open              open the only/cursor repo (else lists the choices)
  $ haw open kernel --print   print the URL instead of spawning a browser")]
    Open {
        /// Repo name (default: the only repo, else list the choices).
        repo: Option<String>,
        /// Print the URL instead of spawning a browser (also the default when stdout isn't a TTY).
        #[arg(long)]
        print: bool,
    },
    /// Open the fleet dashboard (same as bare `haw`).
    #[command(
        alias = "tui",
        after_help = "\
Examples:
  $ haw dash       open the cockpit (identical to running bare `haw`)"
    )]
    Dash {
        /// Drive the cockpit with canned, in-memory data (no workspace/network).
        #[arg(long, hide = true)]
        demo: bool,
    },
    /// Discover, list, and install `haw-*` plugins.
    #[command(after_help = "\
Examples:
  $ haw plugins list                        table of catalog/installed/subscribed plugins
  $ haw plugins list --format json           machine-readable (schema haw.plugins/1)
  $ haw plugins install aspice               cargo-install the first-party `haw-aspice`
  $ haw plugins install aspice --dry-run      print the cargo command without running it
  $ haw plugins path                          print the PATH dirs scanned for `haw-*`

Run `haw plugins <subcommand> --help` for that subcommand's own examples.")]
    Plugins {
        #[command(subcommand)]
        command: PluginsCommand,
    },
    /// Anything else runs a `haw-<name>` plugin from PATH.
    #[command(external_subcommand)]
    Plugin(Vec<String>),
}

#[derive(Subcommand)]
enum PluginsCommand {
    /// List plugins: official catalog, PATH-installed, and manifest-subscribed.
    #[command(after_help = "\
Examples:
  $ haw plugins list                   NAME/STATUS/SUBSCRIBED/DESCRIPTION table
  $ haw plugins list --format json      machine-readable (schema haw.plugins/1)
  $ haw plugins list --remote           also merge the community index (source `remote`)
  $ haw plugins list --remote --index https://example.com/plugins-index.json

STATUS is `installed` when the `haw-<name>` binary is on PATH, else `available`.
SUBSCRIBED shows the phases from the workspace manifest `[plugins]` (if any).
--remote fetches a `haw.plugins.index/1` doc and merges its plugins (source
`remote`); on a network error it warns and falls back to local-only.")]
    List {
        /// `text` (default) or `json` (schema haw.plugins/1).
        #[arg(long, default_value = "text")]
        format: String,
        /// Also fetch and merge the community plugin index.
        #[arg(long)]
        remote: bool,
        /// Community index URL (implies --remote). Default: the first-party index.
        #[arg(long, value_name = "URL")]
        index: Option<String>,
        /// Allow insecure (http://) or internal-IP index URLs when fetching.
        #[arg(long)]
        insecure: bool,
    },
    /// Scaffold a runnable `haw-<name>` plugin skeleton in a new directory.
    #[command(after_help = "\
Examples:
  $ haw plugins new sbom --lang shell            ./haw-sbom/haw-sbom (POSIX sh)
  $ haw plugins new sbom --lang python           ./haw-sbom/haw-sbom + README.md
  $ haw plugins new sbom --lang go               ./haw-sbom/{main.go,go.mod,README.md}
  $ haw plugins new sbom --lang rust             a cargo crate (bin haw-sbom)
  $ haw plugins new sbom --lang shell --dir /tmp/sbom   choose the target dir

Each skeleton implements the plugin contract: reads the `haw.plugin/1` context
from HAW_JSON (falling back to stdin), handles --help and --format json, and
emits a `haw.plugin.report/1` document. Drop it on PATH -> `haw <name>`.
Refuses to overwrite a non-empty target directory.")]
    New {
        /// Plugin name — the verb users type (`haw <name>`).
        name: String,
        /// Skeleton language.
        #[arg(long, value_name = "rust|python|go|shell")]
        lang: PluginLang,
        /// Target directory (default: `./haw-<name>`).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Install a plugin binary via `cargo install`.
    #[command(after_help = "\
Examples:
  $ haw plugins install aspice                       cargo install --git <repo> haw-aspice
  $ haw plugins install aspice --dry-run              print the command, run nothing
  $ haw plugins install haw-custom --git https://example.com/me/plugins   custom source
  $ haw plugins install some-crate --locked           honor the crate's Cargo.lock

The first-party plugins are workspace members (not yet on crates.io), so the
default source is `--git https://github.com/Nastwinns/hawser`. Pass `--git <url>`
to install from a different repository.")]
    Install {
        /// Plugin name (catalog name like `aspice`, or a crate like `haw-foo`).
        name: String,
        /// Install from this git URL instead of the first-party repository.
        #[arg(long, value_name = "URL")]
        git: Option<String>,
        /// Deprecated/no-op: installs are ALWAYS `--locked` for reproducibility.
        #[arg(long, hide = true)]
        locked: bool,
        /// Pin a custom `--git` source to this tag (default source is pinned
        /// to this hawser version automatically).
        #[arg(long, value_name = "TAG", conflicts_with = "rev")]
        tag: Option<String>,
        /// Pin a custom `--git` source to this exact commit.
        #[arg(long, value_name = "SHA", conflicts_with = "tag")]
        rev: Option<String>,
        /// Print the `cargo install` command and exit; run nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the directories scanned for `haw-*` plugins (the PATH entries).
    #[command(after_help = "\
Examples:
  $ haw plugins path       list every PATH dir haw scans for `haw-*` binaries

Drop a `haw-<name>` executable into any of these to make it discoverable.")]
    Path,
}

#[derive(Subcommand)]
enum HooksCommand {
    /// Write a pre-commit hook in every repo that runs `haw verify`.
    #[command(after_help = "\
Examples:
  $ haw hooks install       add the pre-commit hook to every cloned repo")]
    Install,
    /// List the lifecycle hooks the workspace defines.
    #[command(after_help = "\
Examples:
  $ haw hooks list       show pre-sync/post-sync/... hooks declared in haw.toml")]
    List,
}

#[derive(Subcommand)]
enum RepoCommand {
    /// List repos with rev, path, and groups.
    #[command(after_help = "\
Examples:
  $ haw repo list       show every repo's rev, checkout path, and groups")]
    List,
    /// Add a repo to the manifest (keeps your comments and formatting).
    #[command(after_help = "\
Examples:
  $ haw repo add kernel --url git@github.com:acme/kernel.git --rev v6.1.2
  $ haw repo add hal --remote internal --slug hal.git --group firmware
  $ haw repo add app-mqtt --url git@github.com:acme/app-mqtt.git --path apps/mqtt --rev main")]
    Add {
        name: String,
        /// Full clone URL (or use --remote + --slug).
        #[arg(long, conflicts_with_all = ["remote", "slug"])]
        url: Option<String>,
        /// Named remote from [remote.X].
        #[arg(long, requires = "slug")]
        remote: Option<String>,
        /// Repository path under the remote.
        #[arg(long, alias = "repo", requires = "remote")]
        slug: Option<String>,
        #[arg(long, default_value = "main")]
        rev: String,
        /// Checkout path (default: the repo name).
        #[arg(long)]
        path: Option<String>,
        /// Group label (repeatable).
        #[arg(long = "group")]
        groups: Vec<String>,
    },
    /// Remove a repo (refused while a stack or overlay references it).
    #[command(after_help = "\
Examples:
  $ haw repo remove hal       fails if any stack/overlay still references `hal`")]
    Remove { name: String },
}

#[derive(Subcommand)]
enum StackCommand {
    /// List stacks and their repos.
    #[command(after_help = "\
Examples:
  $ haw stack list       show every stack and the repos it composes")]
    List,
    /// Add a stack composed of existing repos.
    #[command(after_help = "\
Examples:
  $ haw stack add gateway --repos kernel,hal,app-mqtt")]
    Add {
        name: String,
        /// Repos in the stack.
        #[arg(
            long = "repos",
            alias = "bricks",
            value_delimiter = ',',
            required = true
        )]
        repos: Vec<String>,
    },
    /// Remove a stack.
    #[command(after_help = "\
Examples:
  $ haw stack remove sensor-node")]
    Remove { name: String },
}

#[derive(Subcommand)]
enum ChangeCommand {
    /// Create one branch across the affected repos.
    #[command(after_help = "\
Examples:
  $ haw change start FEAT-42 --repos kernel,hal      branch two repos
  $ haw change start FEAT-42                          branch every repo in the manifest
  $ haw change start FEAT-42 --skip-branch             adopt each repo's current branch instead
  $ haw change start FEAT-42 --label adas --label perf  labels forwarded to `change request`")]
    Start {
        id: String,
        /// Repos to include (default: all repos in the manifest).
        #[arg(long = "repos", alias = "bricks", value_delimiter = ',')]
        repos: Option<Vec<String>>,
        /// Branch name (default: `change/<id>`).
        #[arg(long)]
        branch: Option<String>,
        /// Adopt each repo's current branch instead of creating one.
        #[arg(long)]
        skip_branch: bool,
        /// Label forwarded to the PR/MRs at `change request` (repeatable).
        #[arg(long = "label")]
        labels: Vec<String>,
        /// `text` (default) or `json` (schema haw.change-start/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Per-repo branch + PR/MR review + CI dashboard for a changeset.
    #[command(after_help = "\
Examples:
  $ haw change status FEAT-42                 branches, dirty state, and PR/MR + CI status
  $ haw change status FEAT-42 --format json    machine-readable (schema haw.change-status/1)")]
    Status {
        id: String,
        /// `text` (default) or `json` (schema haw.change-status/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Push the changeset branches and open cross-linked PR/MRs.
    #[command(after_help = "\
Examples:
  $ haw change request FEAT-42                base branch: the locked branch, else main
  $ haw change request FEAT-42 --base develop   target a specific base branch")]
    Request {
        id: String,
        /// Target branch for the PR/MRs (default: the locked branch, else main).
        #[arg(long)]
        base: Option<String>,
        /// `text` (default) or `json` (schema haw.change-request/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Merge the PR/MRs in dependency order; stops at the first failure.
    #[command(after_help = "\
Examples:
  $ haw change land FEAT-42       merge every repo's PR/MR, in manifest `deps` order")]
    Land {
        id: String,
        /// `text` (default) or `json` (schema haw.change-land/1).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Print a changeset repo's path (usable as: cd "$(haw change goto ID REPO)").
    #[command(after_help = "\
Examples:
  $ haw change goto FEAT-42 kernel            print kernel's checkout path
  $ cd \"$(haw change goto FEAT-42 kernel)\"     cd straight into it
  $ haw change goto FEAT-42                    interactive picker (needs a terminal)")]
    Goto {
        id: String,
        /// Repo name; omit for an interactive picker.
        repo: Option<String>,
    },
    /// Save/restore the multi-repo state of a changeset.
    #[command(after_help = "\
Examples:
  $ haw change snapshot save before-refactor       record every repo's branch + HEAD
  $ haw change snapshot restore before-refactor     check every repo back out
  $ haw change snapshot list                        show saved snapshots")]
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    /// List recorded changesets.
    #[command(after_help = "\
Examples:
  $ haw change list       show every changeset id recorded in .haw/changesets")]
    List,
}

#[derive(Subcommand)]
enum MergeCommand {
    /// Start merging <source> into the current branch; slice the conflicts.
    #[command(after_help = "\
Examples:
  $ haw merge plan origin/feature --repo kernel                merge feature into kernel
  $ haw merge plan release/2.x --repo kernel --into custom-branch   name the integration branch")]
    Plan {
        /// Branch/tag/SHA to merge in.
        source: String,
        /// Repo to merge in (default: the only repo, else required).
        #[arg(long)]
        repo: Option<String>,
        /// Integration branch name (default: haw/merge/<source>).
        #[arg(long)]
        into: Option<String>,
    },
    /// Resolve one slice of the in-progress merge.
    #[command(after_help = "\
Examples:
  $ haw merge resolve src --take theirs --repo kernel    accept the incoming side for `src`
  $ haw merge resolve docs --take ours --repo kernel      keep the current side for `docs`
  $ haw merge resolve src --repo kernel                    stage `src` as you edited it by hand")]
    Resolve {
        slice: String,
        #[arg(long)]
        repo: Option<String>,
        /// Auto-resolve the whole slice to `ours` or `theirs` (else stage as edited).
        #[arg(long)]
        take: Option<TakeSide>,
    },
    /// Show the planned slices and their resolution state.
    #[command(after_help = "\
Examples:
  $ haw merge status --repo kernel       which slices are resolved, which remain")]
    Status {
        #[arg(long)]
        repo: Option<String>,
    },
    /// Seal the merge: commit it, fast-forward the target, drop temp branches.
    #[command(after_help = "\
Examples:
  $ haw merge cleanup --repo kernel                        refuses if any slice is unresolved
  $ haw merge cleanup --repo kernel -m 'merge feature'      custom merge commit message")]
    Cleanup {
        #[arg(long)]
        repo: Option<String>,
        /// Merge commit message (default: git's merge message).
        #[arg(long, short = 'm')]
        message: Option<String>,
    },
    /// Abort the planned merge and restore the target branch.
    #[command(after_help = "\
Examples:
  $ haw merge abort --repo kernel       undo the merge, drop the integration branch")]
    Abort {
        #[arg(long)]
        repo: Option<String>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum TakeSide {
    Ours,
    Theirs,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum PluginLang {
    Rust,
    Python,
    Go,
    Shell,
}

#[derive(Subcommand)]
enum SnapshotCommand {
    /// Record every repo's branch + HEAD under a name.
    #[command(after_help = "\
Examples:
  $ haw change snapshot save before-refactor")]
    Save { name: String },
    /// Check every repo back out to a saved state (refuses on dirty repos).
    #[command(after_help = "\
Examples:
  $ haw change snapshot restore before-refactor")]
    Restore { name: String },
    /// List saved snapshots.
    #[command(after_help = "\
Examples:
  $ haw change snapshot list")]
    List,
}

fn main() -> ExitCode {
    // Behave like a well-mannered CLI under `| head`: die on SIGPIPE instead of
    // panicking on a broken stdout (e.g. `haw completions bash | head`).
    sigpipe::reset();
    match run() {
        Ok(code) => code,
        Err(err) => {
            let c = Palette::new();
            eprintln!("{} {err}", c.err("error:"));
            for cause in err.chain().skip(1) {
                eprintln!("  {} {cause}", c.dim("↳"));
            }
            if let Some(hint) = hint_for(&format!("{err:#}").to_lowercase()) {
                eprintln!("\n{} {hint}", c.bold("hint:"));
            }
            eprintln!(
                "\nRun {} for usage, or {} for a command's options and examples.",
                c.bold("`haw --help`"),
                c.bold("`haw <command> --help`")
            );
            ExitCode::FAILURE
        }
    }
}

/// A one-line actionable hint for common failures, matched on the error text.
fn hint_for(msg: &str) -> Option<&'static str> {
    if msg.contains("haw.toml") || msg.contains("manifest") || msg.contains("no such file") {
        Some(
            "no workspace here — run `haw init <manifest-url|path>`, cd into a workspace, \
              or point at one with `--manifest <path>`.",
        )
    } else if msg.contains("token") {
        Some(
            "set a forge token: HAW_GITHUB_TOKEN / GITHUB_TOKEN (or run `gh auth login`), \
              or HAW_GITLAB_TOKEN / GITLAB_TOKEN for GitLab.",
        )
    } else if msg.contains("no stack") || msg.contains("select a stack") {
        Some("pass `--stack <name>` or run `haw switch <stack>`; list them with `haw stack list`.")
    } else if msg.contains("lock") || msg.contains("drift") {
        Some("run `haw sync` (or `haw lock`) to resolve; `haw verify` reports drift vs haw.lock.")
    } else if msg.contains("not a git repo") || msg.contains("not cloned") {
        Some("run `haw sync` to clone the repos declared in haw.toml first.")
    } else {
        None
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let _ = MANIFEST_ARG.set(cli.manifest.clone());
    let Some(command) = cli.command else {
        dash(false)?;
        return Ok(ExitCode::SUCCESS);
    };
    match command {
        Command::Init { source, insecure } => init(&source, insecure)?,
        Command::Sync {
            locked,
            stack,
            overlay,
            groups,
            shared,
            filter,
            depth,
            recurse_submodules,
            jobs,
        } => sync(
            stack.as_deref(),
            &overlay,
            &groups,
            shared,
            locked,
            filter,
            depth,
            recurse_submodules,
            jobs,
        )?,
        Command::Lock { overlay, format } => lock(&overlay, &format)?,
        Command::Pin => pin()?,
        Command::Unpin { overlay } => unpin(&overlay)?,
        Command::Repo { command } => match command {
            RepoCommand::List => repo_list()?,
            RepoCommand::Add {
                name,
                url,
                remote,
                slug,
                rev,
                path,
                groups,
            } => repo_add(&name, url, remote, slug, rev, path, groups)?,
            RepoCommand::Remove { name } => repo_remove(&name)?,
        },
        Command::Stack { command } => match command {
            StackCommand::List => stack_list()?,
            StackCommand::Add { name, repos } => stack_add(&name, &repos)?,
            StackCommand::Remove { name } => stack_remove(&name)?,
        },
        Command::Status {
            groups,
            format,
            verify,
        } => return status(&groups, &format, verify),
        Command::Switch {
            stack,
            filter,
            depth,
            jobs,
        } => switch(&stack, filter, depth, jobs)?,
        Command::Tree {
            stack,
            overlay,
            format,
        } => tree(&cli.manifest, stack.as_deref(), &overlay, &format)?,
        Command::Run {
            command,
            command_flag,
            groups,
            jobs,
            format,
        } => {
            let cmd = command
                .or(command_flag)
                .context("pass the command: haw run 'git fetch'")?;
            return run_across(&cmd, &groups, jobs, &format);
        }
        Command::Grep {
            pattern,
            stack,
            json,
        } => grep_across(&pattern, stack.as_deref(), json)?,
        Command::Change { command } => match command {
            ChangeCommand::Start {
                id,
                repos,
                branch,
                skip_branch,
                labels,
                format,
            } => change_start(
                &id,
                repos.as_deref(),
                branch.as_deref(),
                skip_branch,
                &labels,
                &format,
            )?,
            ChangeCommand::Status { id, format } => change_status(&id, &format)?,
            ChangeCommand::Request { id, base, format } => {
                return change_request(&id, base.as_deref(), &format);
            }
            ChangeCommand::Land { id, format } => return change_land(&id, &format),
            ChangeCommand::Goto { id, repo } => change_goto(&id, repo.as_deref())?,
            ChangeCommand::Snapshot { command } => match command {
                SnapshotCommand::Save { name } => snapshot_save(&name)?,
                SnapshotCommand::Restore { name } => snapshot_restore(&name)?,
                SnapshotCommand::List => snapshot_list()?,
            },
            ChangeCommand::List => change_list()?,
        },
        Command::Verify { format } => return verify(&format),
        Command::Build {
            groups,
            jobs,
            format,
        } => return build_or_test(true, &groups, jobs, &format),
        Command::Test {
            groups,
            jobs,
            format,
        } => return build_or_test(false, &groups, jobs, &format),
        Command::Open { repo, print } => return open_cmd(repo.as_deref(), print),
        Command::Hooks { command } => match command {
            HooksCommand::Install => hooks_install()?,
            HooksCommand::List => hooks_list()?,
        },
        Command::Evidence { out } => evidence(&out)?,
        Command::Publish {
            files,
            to,
            name,
            version,
            url,
            dry_run,
            insecure,
            format,
        } => {
            return publish_cmd(
                &files,
                &to,
                name.as_deref(),
                version.as_deref(),
                url.as_deref(),
                dry_run,
                insecure,
                format.as_deref(),
            );
        }
        Command::Import { from } => import_manifest(&from)?,
        Command::Merge { command } => match command {
            MergeCommand::Plan { source, repo, into } => {
                merge_plan(&source, repo.as_deref(), into.as_deref())?
            }
            MergeCommand::Resolve { slice, repo, take } => {
                merge_resolve(&slice, repo.as_deref(), take)?
            }
            MergeCommand::Status { repo } => merge_status(repo.as_deref())?,
            MergeCommand::Cleanup { repo, message } => {
                merge_cleanup(repo.as_deref(), message.as_deref())?
            }
            MergeCommand::Abort { repo } => merge_abort(repo.as_deref())?,
        },
        Command::Completions { shell } => completions(shell),
        Command::Dash { demo } => dash(demo)?,
        Command::Plugins { command } => match command {
            PluginsCommand::List {
                format,
                remote,
                index,
                insecure,
            } => plugins_list(
                &format,
                remote || index.is_some(),
                index.as_deref(),
                insecure,
            )?,
            PluginsCommand::New { name, lang, dir } => plugins_new(&name, lang, dir.as_deref())?,
            PluginsCommand::Install {
                name,
                git,
                locked: _,
                tag,
                rev,
                dry_run,
            } => {
                let pin = match (tag, rev) {
                    (Some(t), _) => Some(GitPin::Tag(t)),
                    (_, Some(r)) => Some(GitPin::Rev(r)),
                    _ => None,
                };
                return plugins_install(&name, git.as_deref(), pin, dry_run);
            }
            PluginsCommand::Path => plugins_path(),
        },
        Command::Plugin(args) => return plugin(&args),
    }
    Ok(ExitCode::SUCCESS)
}

/// The `--manifest` flag, captured once in `run()` so every command —
/// including the bare `dash`/TUI entrypoint — honors it, not just `tree`.
static MANIFEST_ARG: OnceLock<PathBuf> = OnceLock::new();

/// Resolve `--manifest` (default `haw.toml`) against the current directory.
fn manifest_path() -> Result<PathBuf> {
    let manifest = MANIFEST_ARG
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from(MANIFEST_FILE));
    Ok(if manifest.is_absolute() {
        manifest
    } else {
        std::env::current_dir()?.join(manifest)
    })
}

pub(crate) fn open_workspace() -> Result<Workspace> {
    Ok(Workspace::open_manifest(manifest_path()?)?)
}

pub(crate) fn default_jobs(flag: Option<usize>) -> usize {
    flag.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(4)
            .min(8)
    })
}

/// Resolve clone tuning as CLI-flag-over-manifest-`[defaults]`. A present CLI
/// flag wins; otherwise the manifest default (if any) applies. `filter` and
/// `depth` resolve independently. `recurse_submodules` (when true) overrides
/// every repo to recurse submodules; `false` leaves each repo's own setting
/// (per-repo `submodules` OR `[defaults] submodules`, applied in the resolver).
pub(crate) fn resolve_tuning(
    ws: &Workspace,
    filter: Option<String>,
    depth: Option<u32>,
    recurse_submodules: bool,
) -> CloneTuning {
    CloneTuning {
        filter: filter.or_else(|| ws.manifest.defaults.filter.clone()),
        depth: depth.or(ws.manifest.defaults.depth),
        submodules: recurse_submodules.then_some(true),
    }
}

pub(crate) fn record(
    ws: &Workspace,
    op: &str,
    repo: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
) {
    if let Err(err) = audit::record(ws, op, repo, before, after) {
        eprintln!("warning: audit log not written: {err}");
    }
}

fn init(source: &str, insecure: bool) -> Result<()> {
    let dest = manifest_path()?;
    if dest.exists() {
        bail!("{} already exists here", dest.display());
    }
    // Treat only explicit URL schemes as remote; a local PATH still works
    // unchanged. Remote fetches are https-only and IP-hardened (see
    // `check_fetch_url`) unless `--insecure` is passed.
    let text = if source.starts_with("http://")
        || source.starts_with("https://")
        || source.contains("://")
    {
        check_fetch_url(source, insecure)?;
        let client = hardened_client()?;
        fetch_text_capped(&client, source)?
    } else {
        let path = Path::new(source);
        if !path.is_file() {
            bail!("{source} is not a file or URL");
        }
        std::fs::read_to_string(path)?
    };
    text.parse::<haw_core::manifest::Manifest>()
        .with_context(|| format!("{source} is not a valid manifest"))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&dest, text)?;
    println!("initialized workspace from {source}");
    println!("next: haw sync");
    Ok(())
}

fn lock_json(lockfile: &haw_core::lock::Lockfile) -> serde_json::Value {
    json!({
        "schema": "haw.lock/1",
        "repos": lockfile.repos.iter().map(|r| json!({
            "name": r.name,
            "url": r.url,
            "path": r.path.to_string_lossy(),
            "rev": r.rev,
            "source_rev": r.source_rev,
            "branch": r.branch,
            "groups": r.groups,
        })).collect::<Vec<_>>(),
    })
}

fn lock(overlays: &[String], format: &str) -> Result<()> {
    if !matches!(format, "text" | "json") {
        bail!("unknown format `{format}` (use text or json)");
    }
    let ws = open_workspace()?;
    let backend = ShellGit;
    hooks::fire(&ws, hooks::Hook::PreLock, &json!({"overlays": overlays}))?;
    let lockfile = ws.make_lock(overlays, &backend)?;
    lockfile.save(&ws.lock_path())?;
    hooks::fire(&ws, hooks::Hook::PostLock, &json!({"overlays": overlays}))?;
    record(&ws, "lock.write", None, None, None);

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&lock_json(&lockfile))?);
        return Ok(());
    }

    let c = Palette::new();
    println!(
        "{}",
        c.bold(&format!(
            "wrote haw.lock ({} repos pinned)",
            lockfile.repos.len()
        ))
    );
    let width = lockfile
        .repos
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(4);
    for repo in &lockfile.repos {
        println!(
            "  {}  {}  {} {}",
            c.name(&format!("{:<width$}", repo.name)),
            c.dim(&repo.rev[..12.min(repo.rev.len())]),
            c.dim("<-"),
            c.rev(&repo.source_rev)
        );
    }
    Ok(())
}

fn pin() -> Result<()> {
    let ws = open_workspace()?;
    let lockfile = ws.pin(&ShellGit)?;
    lockfile.save(&ws.lock_path())?;
    record(&ws, "lock.pin", None, None, None);
    let c = Palette::new();
    println!(
        "{}",
        c.bold(&format!(
            "pinned haw.lock to current HEADs ({} repos)",
            lockfile.repos.len()
        ))
    );
    let width = lockfile
        .repos
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(4);
    for repo in &lockfile.repos {
        println!(
            "  {}  {}  {}",
            c.name(&format!("{:<width$}", repo.name)),
            c.dim(&repo.rev[..8.min(repo.rev.len())]),
            c.rev(&format!("({})", repo.branch))
        );
    }
    Ok(())
}

fn unpin(overlays: &[String]) -> Result<()> {
    lock(overlays, "text")?;
    println!("restored haw.lock to the manifest revs");
    Ok(())
}

fn repo_list() -> Result<()> {
    let ws = open_workspace()?;
    if ws.manifest.repos.is_empty() {
        println!("no repos — add one with `haw repo add <name> --url <url>`");
        return Ok(());
    }
    let c = Palette::new();
    let width = ws.manifest.repos.keys().map(String::len).max().unwrap_or(4);
    for (name, repo) in &ws.manifest.repos {
        let groups = if repo.groups.is_empty() {
            String::new()
        } else {
            format!("  [{}]", repo.groups.join(", "))
        };
        println!(
            "{}  {}  {}{}",
            c.name(&format!("{name:<width$}")),
            c.rev(&repo.rev),
            c.dim(&repo.checkout_path(name).display().to_string()),
            c.dim(&groups)
        );
    }
    Ok(())
}

fn repo_add(
    name: &str,
    url: Option<String>,
    remote: Option<String>,
    slug: Option<String>,
    rev: String,
    path: Option<String>,
    groups: Vec<String>,
) -> Result<()> {
    let ws = open_workspace()?;
    let spec = edit::NewRepo {
        name: name.to_string(),
        url,
        remote,
        repo: slug,
        rev,
        path,
        groups,
    };
    let text = std::fs::read_to_string(ws.manifest_path())?;
    let updated = edit::add_repo(&text, &spec)?;
    std::fs::write(ws.manifest_path(), updated)?;
    record(&ws, "repo.add", Some(name), None, None);
    println!("added repo `{name}`");
    println!("next: haw lock && haw sync");
    Ok(())
}

fn repo_remove(name: &str) -> Result<()> {
    let ws = open_workspace()?;
    let text = std::fs::read_to_string(ws.manifest_path())?;
    let updated = edit::remove_repo(&text, name)?;
    std::fs::write(ws.manifest_path(), updated)?;
    record(&ws, "repo.remove", Some(name), None, None);
    println!("removed repo `{name}` from the manifest");
    println!("note: its clone stays on disk; delete the directory if unwanted");
    Ok(())
}

fn stack_list() -> Result<()> {
    let ws = open_workspace()?;
    if ws.manifest.stacks.is_empty() {
        println!("no stacks — add one with `haw stack add <name> --repos a,b`");
        return Ok(());
    }
    let c = Palette::new();
    let current = ws.current_stack();
    for (name, stack) in &ws.manifest.stacks {
        let marker = if current.as_deref() == Some(name) {
            c.ok("*")
        } else {
            " ".to_string()
        };
        println!(
            "{marker} {}: {}",
            c.name(name),
            c.rev(&stack.repos.join(", "))
        );
    }
    Ok(())
}

fn stack_add(name: &str, repos: &[String]) -> Result<()> {
    let ws = open_workspace()?;
    let text = std::fs::read_to_string(ws.manifest_path())?;
    let updated = edit::add_stack(&text, name, repos)?;
    std::fs::write(ws.manifest_path(), updated)?;
    record(&ws, "stack.add", Some(name), None, None);
    println!("added stack `{name}` ({} repos)", repos.len());
    Ok(())
}

fn stack_remove(name: &str) -> Result<()> {
    let ws = open_workspace()?;
    let text = std::fs::read_to_string(ws.manifest_path())?;
    let updated = edit::remove_stack(&text, name)?;
    std::fs::write(ws.manifest_path(), updated)?;
    record(&ws, "stack.remove", Some(name), None, None);
    println!("removed stack `{name}`");
    Ok(())
}

fn status_json(statuses: &[RepoStatus]) -> serde_json::Value {
    json!({
        "schema": "haw.status/1",
        "repos": statuses.iter().map(|s| json!({
            "name": s.name,
            "path": s.path.to_string_lossy(),
            "missing": s.missing,
            "branch": s.branch,
            "head": s.head,
            "dirty": s.dirty,
            "locked_rev": s.locked_rev,
            "drift": s.drift,
            "ahead_behind": s.ahead_behind.map(|(a, b)| json!({"ahead": a, "behind": b})),
            "groups": s.groups,
        })).collect::<Vec<_>>(),
    })
}

fn status(groups: &[String], format: &str, verify: bool) -> Result<ExitCode> {
    let ws = open_workspace()?;
    let statuses = ws.status(groups, &ShellGit)?;
    let failing = statuses.iter().any(|s| s.missing || s.dirty || s.drift);

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&status_json(&statuses))?),
        "text" => {
            if statuses.is_empty() {
                println!("no matching repos");
            } else {
                let c = Palette::new();
                let width = statuses
                    .iter()
                    .map(|s| s.name.len())
                    .max()
                    .unwrap_or(4)
                    .max(4);
                println!(
                    "{}",
                    c.header(&format!(
                        "{:<width$}  {:<24} {:<10} {:<6} DRIFT",
                        "REPO", "BRANCH", "HEAD", "DIRTY"
                    ))
                );
                for s in &statuses {
                    if s.missing {
                        println!(
                            "{}  {}",
                            c.name(&format!("{:<width$}", s.name)),
                            c.dim("(not cloned — run `haw sync`)")
                        );
                        continue;
                    }
                    let name = if s.dirty || s.drift {
                        c.warn(&format!("{:<width$}", s.name))
                    } else {
                        c.name(&format!("{:<width$}", s.name))
                    };
                    println!(
                        "{name}  {}  {} {} {}",
                        c.rev(&format!(
                            "{:<24}",
                            s.branch.as_deref().unwrap_or("(detached)")
                        )),
                        c.dim(&format!(
                            "{:<10}",
                            s.head
                                .as_deref()
                                .map(|h| &h[..8.min(h.len())])
                                .unwrap_or("—")
                        )),
                        if s.dirty {
                            c.warn(&format!("{:<6}", "yes"))
                        } else {
                            c.ok(&format!("{:<6}", "-"))
                        },
                        if s.drift { c.err("YES") } else { c.ok("-") },
                    );
                }
            }
        }
        other => bail!("unknown format `{other}` (use text or json)"),
    }
    if verify && failing {
        return Ok(ExitCode::from(3));
    }
    Ok(ExitCode::SUCCESS)
}

fn switch(
    stack: &str,
    filter: Option<String>,
    depth: Option<u32>,
    jobs: Option<usize>,
) -> Result<()> {
    let ws = open_workspace()?;
    let stack = ws.pick_stack(Some(stack))?;
    ws.set_current_stack(&stack)?;
    record(&ws, "switch", None, None, Some(&stack));
    hooks::fire(&ws, hooks::Hook::PostSwitch, &json!({"stack": stack}))?;
    println!("switched to stack `{stack}`");
    sync(
        Some(&stack),
        &[],
        &[],
        false,
        false,
        filter,
        depth,
        false,
        jobs,
    )
}

fn tree(path: &Path, stack: Option<&str>, overlays: &[String], format: &str) -> Result<()> {
    let manifest = TomlLoader.load(path)?;
    let selected: Vec<String> = match stack {
        Some(name) => vec![name.to_string()],
        None => manifest.stacks.keys().cloned().collect(),
    };
    if selected.is_empty() {
        println!("no stacks defined in {}", path.display());
        return Ok(());
    }

    if format == "json" {
        let mut stacks = Vec::with_capacity(selected.len());
        for name in &selected {
            let resolution = resolver::resolve(&manifest, name, overlays)?;
            stacks.push(json!({
                "name": name,
                "repos": resolution.repos.iter().map(|r| json!({
                    "name": r.name,
                    "rev": r.rev,
                    "url": r.url,
                    "path": r.path.to_string_lossy(),
                })).collect::<Vec<_>>(),
            }));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"schema": "haw.tree/1", "stacks": stacks}))?
        );
        return Ok(());
    }
    if format != "text" {
        bail!("unknown format `{format}` (use text or json)");
    }

    let c = Palette::new();
    println!("{}", c.paint("2", &path.display().to_string()));
    for (i, name) in selected.iter().enumerate() {
        let resolution = resolver::resolve(&manifest, name, overlays)?;
        let last_stack = i == selected.len() - 1;
        let branch = if last_stack { "└─" } else { "├─" };
        println!("{} {}", c.paint("2", branch), c.paint("1;36", name));

        let stem = if last_stack { "   " } else { "│  " };
        let width = resolution
            .repos
            .iter()
            .map(|b| b.name.len())
            .max()
            .unwrap_or(0);
        for (j, repo) in resolution.repos.iter().enumerate() {
            let tee = if j == resolution.repos.len() - 1 {
                "└─"
            } else {
                "├─"
            };
            println!(
                "{}{} {}  {}  {}",
                c.paint("2", stem),
                c.paint("2", tee),
                format_args!("{:<width$}", repo.name),
                c.paint("33", &repo.rev),
                c.paint("2", &format!("({})", repo.url)),
            );
        }
    }
    Ok(())
}

/// Write the completion script for `shell` to stdout, built from the clap
/// command tree so it always tracks the real flags and subcommands.
fn completions(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
}

fn run_across(
    command: &str,
    groups: &[String],
    jobs: Option<usize>,
    format: &str,
) -> Result<ExitCode> {
    if format != "text" && format != "json" {
        bail!("unknown format `{format}` (use text or json)");
    }
    let as_json = format == "json";
    let ws = open_workspace()?;
    let backend = ShellGit;
    let repos: Vec<(String, PathBuf)> = match ws.read_lock()? {
        Some(lock) => lock
            .repos
            .iter()
            .filter(|b| resolver::group_match(&b.groups, groups))
            .map(|b| (b.name.clone(), ws.root.join(&b.path)))
            .collect(),
        None => ws
            .manifest
            .repos
            .iter()
            .filter(|(_, repo)| resolver::group_match(&repo.groups, groups))
            .map(|(name, repo)| (name.clone(), ws.root.join(repo.checkout_path(name))))
            .collect(),
    };
    let present: Vec<(String, PathBuf)> = repos
        .into_iter()
        .filter(|(_, path)| backend.is_repo(path))
        .collect();
    if present.is_empty() {
        bail!("no cloned repos — run `haw sync` first");
    }

    let results = fan_out(&present, default_jobs(jobs), |(name, path)| {
        let output = shell_command(command).current_dir(path).output();
        (name.clone(), output)
    });

    let total = results.len();
    let mut failures = 0usize;
    if as_json {
        let repos = results
            .iter()
            .map(|(name, output)| match output {
                Ok(out) => {
                    let ok = out.status.success();
                    if !ok {
                        failures += 1;
                    }
                    json!({"name": name, "exit_code": out.status.code(), "ok": ok})
                }
                Err(err) => {
                    failures += 1;
                    json!({"name": name, "exit_code": null, "ok": false, "error": err.to_string()})
                }
            })
            .collect::<Vec<_>>();
        let value = json!({
            "schema": "haw.run/1",
            "command": command,
            "repos": repos,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(fail_exit(failures));
    }

    let c = Palette::new();
    for (name, output) in results {
        println!("{} {} {}", c.dim("──"), c.name(&name), c.dim("──"));
        match output {
            Ok(out) => {
                print!("{}", String::from_utf8_lossy(&out.stdout));
                eprint!("{}", String::from_utf8_lossy(&out.stderr));
                if !out.status.success() {
                    failures += 1;
                    eprintln!("(exit: {})", out.status);
                }
            }
            Err(err) => {
                failures += 1;
                eprintln!("(failed to run: {err})");
            }
        }
    }
    println!("ran in {}/{} repos", total - failures, total);
    if failures > 0 {
        bail!("command failed in {failures} repo(s)");
    }
    Ok(ExitCode::SUCCESS)
}

/// `ExitCode::FAILURE` when any repo failed, else success. Keeps the
/// "one machine doc, still exit non-zero on failure" contract for `--format json`.
pub(crate) fn fail_exit(failures: usize) -> ExitCode {
    if failures > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn grep_across(pattern: &str, stack: Option<&str>, json: bool) -> Result<()> {
    let ws = open_workspace()?;
    let repos = fleet_repos(&ws, stack)?;
    if repos.is_empty() {
        bail!("no cloned repos — run `haw sync` first");
    }
    let results = fan_out(&repos, default_jobs(None), |(name, path)| {
        (name.clone(), git_grep(path, pattern))
    });

    let mut hits: Vec<haw_tui::GrepHit> = Vec::new();
    for (name, out) in &results {
        for line in out.lines() {
            if let Some(hit) = haw_tui::parse_grep_line(name, line) {
                hits.push(hit);
            }
        }
    }

    if json {
        let value = json!({
            "schema": "haw.grep/1",
            "pattern": pattern,
            "hits": hits.iter().map(|h| json!({
                "repo": h.repo,
                "path": h.path,
                "line": h.line,
                "text": h.text,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }

    let c = Palette::new();
    let mut total = 0usize;
    for (name, _) in &results {
        let repo_hits: Vec<&haw_tui::GrepHit> = hits.iter().filter(|h| &h.repo == name).collect();
        if repo_hits.is_empty() {
            continue;
        }
        total += repo_hits.len();
        println!(
            "{} {}",
            c.name(name),
            c.dim(&format!("({} hit(s))", repo_hits.len()))
        );
        for hit in repo_hits {
            println!(
                "  {}:{}:{}",
                c.dim(&hit.path),
                c.rev(&hit.line.to_string()),
                hit.text.trim_end()
            );
        }
    }
    println!(
        "{}",
        c.bold(&format!(
            "{total} hit(s) in {} repo(s) for `{pattern}`",
            results.len()
        ))
    );
    Ok(())
}

#[cfg(windows)]
pub(crate) fn shell_command(command: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("cmd");
    cmd.arg("/C").arg(command);
    cmd
}

#[cfg(not(windows))]
pub(crate) fn shell_command(command: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd
}

/// The forge homepage (`https://host/owner/repo`) for a manifest repo, derived
/// from its clone URL. `None` when the repo has no resolvable remote or the URL
/// isn't a recognizable forge coordinate.
fn repo_forge_url(ws: &Workspace, name: &str) -> Option<String> {
    let url = ws
        .manifest
        .repos
        .get(name)?
        .clone_url(&ws.manifest.remotes)?;
    let coords = haw_forge::repo_coords(&url)?;
    Some(format!("https://{}/{}", coords.host, coords.path))
}

fn open_cmd(repo: Option<&str>, print: bool) -> Result<ExitCode> {
    let ws = open_workspace()?;
    // Which repo? An explicit arg, else the only repo, else ask the caller to pick.
    let name = match repo {
        Some(name) => {
            if !ws.manifest.repos.contains_key(name) {
                bail!("repo `{name}` is not in the manifest");
            }
            name.to_string()
        }
        None => {
            let names: Vec<&String> = ws.manifest.repos.keys().collect();
            match names.as_slice() {
                [only] => (*only).to_string(),
                [] => bail!("no repos in the manifest"),
                _ => {
                    eprintln!("pick a repo: haw open <repo>");
                    for name in names {
                        eprintln!("  {name}");
                    }
                    return Ok(ExitCode::from(2));
                }
            }
        }
    };
    let url = repo_forge_url(&ws, &name)
        .with_context(|| format!("repo `{name}` has no forge URL (check its remote)"))?;

    // Print (never spawn) when asked, or when stdout isn't a TTY (scripts/CI).
    if print || !std::io::stdout().is_terminal() {
        println!("{url}");
        return Ok(ExitCode::SUCCESS);
    }
    match haw_tui::open_in_browser(&url) {
        Ok(()) => {
            println!("→ opened {url}");
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => bail!("open failed: {err}"),
    }
}

fn verify(format: &str) -> Result<ExitCode> {
    let ws = open_workspace()?;
    if !ws.lock_path().exists() {
        bail!("no haw.lock to verify against — run `haw lock` first");
    }
    let statuses = ws.status(&[], &ShellGit)?;
    let offenders: Vec<&RepoStatus> = statuses
        .iter()
        .filter(|s| s.missing || s.dirty || s.drift)
        .collect();

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&status_json(&statuses))?);
    } else {
        for s in &offenders {
            let why = if s.missing {
                "not cloned"
            } else if s.dirty {
                "dirty"
            } else {
                "drift (head != lock)"
            };
            println!("  ✗ {}  {why}", s.name);
        }
    }
    if offenders.is_empty() {
        if format != "json" {
            println!("verified: tree matches haw.lock ({} repos)", statuses.len());
        }
        Ok(ExitCode::SUCCESS)
    } else {
        if format != "json" {
            eprintln!(
                "verify failed: {} repo(s) diverge from haw.lock",
                offenders.len()
            );
        }
        Ok(ExitCode::from(3))
    }
}

fn hooks_install() -> Result<()> {
    let ws = open_workspace()?;
    let backend = ShellGit;
    let script = "#!/bin/sh\n# installed by `haw hooks install`\nhaw verify || {\n  echo 'haw: tree diverges from haw.lock (run haw sync or haw pin)' >&2\n  exit 1\n}\n";
    let mut installed = 0usize;
    for (name, repo) in &ws.manifest.repos {
        let path = ws.root.join(repo.checkout_path(name));
        if !backend.is_repo(&path) {
            continue;
        }
        let hook = path.join(".git").join("hooks").join("pre-commit");
        std::fs::write(&hook, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))?;
        }
        installed += 1;
        println!("  ✓ {name}  pre-commit -> haw verify");
    }
    if installed == 0 {
        bail!("no cloned repos — run `haw sync` first");
    }
    println!("installed the integrity pre-commit in {installed} repo(s)");
    Ok(())
}

fn hooks_list() -> Result<()> {
    let ws = open_workspace()?;
    let dir = ws.state_dir().join("hooks");
    let mut any = false;
    for hook in hooks::Hook::ALL {
        let name = hook.name();
        let path = dir.join(name);
        if path.exists() {
            any = true;
            println!("  {name}  {}", path.display());
        }
    }
    if !any {
        println!(
            "no lifecycle hooks — add executables under {}",
            dir.display()
        );
    }
    Ok(())
}

fn evidence(out: &Path) -> Result<()> {
    let ws = open_workspace()?;
    let staging = ws.state_dir().join("evidence");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    std::fs::copy(ws.manifest_path(), staging.join(MANIFEST_FILE))?;
    if ws.lock_path().exists() {
        std::fs::copy(ws.lock_path(), staging.join("haw.lock"))?;
    }
    let audit_log = ws.state_dir().join("audit.jsonl");
    if audit_log.exists() {
        std::fs::copy(&audit_log, staging.join("audit.jsonl"))?;
    }
    let statuses = ws.status(&[], &ShellGit)?;
    std::fs::write(
        staging.join("status.json"),
        serde_json::to_string_pretty(&status_json(&statuses))?,
    )?;
    std::fs::write(
        staging.join("tool.json"),
        serde_json::to_string_pretty(&json!({
            "schema": "haw.evidence/1",
            "tool": "haw",
            "version": env!("CARGO_PKG_VERSION"),
        }))?,
    )?;

    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(std::env::current_dir()?.join(out))
        .arg("-C")
        .arg(&staging)
        .arg(".")
        .status()?;
    if !status.success() {
        bail!("tar failed while writing {}", out.display());
    }
    let _ = std::fs::remove_dir_all(&staging);
    record(
        &ws,
        "evidence",
        None,
        None,
        Some(&out.display().to_string()),
    );
    println!("wrote evidence bundle {}", out.display());
    Ok(())
}

/// Fire a lifecycle phase: run the `.haw/hooks/<phase>` script (if any) and
/// dispatch every `[plugins]` entry subscribed to it.
///
/// `pre-*` failures return `Err` (the caller aborts); `post-*` failures are
/// printed as warnings and swallowed. Missing plugin binaries are skipped
/// (fail-open). `extra` is merged into the plugin context for diagnostics.
pub(crate) fn fire_phase(
    ws: &Workspace,
    hook: hooks::Hook,
    extra: serde_json::Value,
) -> Result<()> {
    let is_pre = hook.is_pre();

    match hooks::fire(ws, hook, &extra) {
        Ok(()) => {}
        Err(err) if is_pre => return Err(err.into()),
        Err(err) => eprintln!("  ! {} hook: {err} (continuing)", hook.name()),
    }

    let subscriptions = &ws.manifest.plugins;
    if subscriptions.is_empty() {
        return Ok(());
    }

    let repos: Vec<RepoContext> = ws
        .manifest
        .repos
        .iter()
        .map(|(name, repo)| RepoContext {
            name: name.clone(),
            path: ws.root.join(repo.checkout_path(name)),
            rev: repo.rev.clone(),
            groups: repo.groups.clone(),
        })
        .collect();
    let mut context =
        plugin::phase_context(&ws.root, ws.current_stack().as_deref(), &repos, hook.name());
    if let (Some(obj), serde_json::Value::Object(extra)) = (context.as_object_mut(), extra) {
        for (key, value) in extra {
            obj.entry(key).or_insert(value);
        }
    }

    let c = Palette::new();
    let dispatches = plugin::dispatch(&ProcessRunner, subscriptions, hook.name(), &context);
    let mut blocked: Vec<String> = Vec::new();
    for dispatch in dispatches {
        match dispatch {
            Dispatch::Ran(report) => {
                let mark = if report.ok { c.ok("✓") } else { c.err("✗") };
                println!(
                    "  {mark} {} {}",
                    c.name(&report.plugin),
                    c.dim(&report.summary)
                );
                for finding in &report.findings {
                    println!("      [{}] {}", finding.level, finding.message);
                }
                if !report.ok && is_pre {
                    blocked.push(report.plugin);
                }
            }
            Dispatch::Missing { plugin } => {
                eprintln!(
                    "  {} {plugin} (no haw-{plugin} on PATH — skipped)",
                    c.dim("·")
                );
            }
            Dispatch::Unparseable { plugin, detail } => {
                eprintln!("  {} {plugin}: {detail}", c.err("!"));
            }
        }
    }
    if !blocked.is_empty() {
        bail!(
            "{} plugin(s) vetoed `{}`: {}",
            blocked.len(),
            hook.name(),
            blocked.join(", ")
        );
    }
    Ok(())
}

/// The first-party plugins shipped in this repository. `name` is the bare
/// subcommand (`haw <name>` / `haw-<name>`), `krate` the workspace crate.
struct CatalogPlugin {
    name: &'static str,
    krate: &'static str,
    description: &'static str,
}

/// The official catalog. Kept hardcoded (not on crates.io yet); the default
/// `haw plugins install` source is the first-party repository below.
const PLUGIN_CATALOG: &[CatalogPlugin] = &[
    CatalogPlugin {
        name: "aspice",
        krate: "haw-aspice",
        description: "ASPICE/qualification traceability from the pinned fleet",
    },
    CatalogPlugin {
        name: "jira",
        krate: "haw-jira",
        description: "link a changeset to a Jira issue and transition it on land",
    },
    CatalogPlugin {
        name: "misra",
        krate: "haw-misra",
        description: "MISRA C static-analysis gate (cppcheck) for pre-request",
    },
    CatalogPlugin {
        name: "compliance",
        krate: "haw-compliance",
        description: "SBOM (CycloneDX + SPDX) generation",
    },
    CatalogPlugin {
        name: "artifact",
        krate: "haw-artifact",
        description: "SLSA/in-toto provenance + cosign/minisign signing",
    },
    CatalogPlugin {
        name: "git-gate",
        krate: "haw-git-gate",
        description: "secret / hygiene pre-commit & lifecycle gate",
    },
    // Starter example plugins (scripts under examples/plugins/, not crates).
    // `krate` doubles as the example binary name for display/discovery.
    CatalogPlugin {
        name: "fleet-status",
        krate: "haw-fleet-status",
        description: "compact per-repo health panel (branch, dirty?, ahead/behind)",
    },
    CatalogPlugin {
        name: "docker",
        krate: "haw-docker",
        description: "report Dockerfile/compose assets per repo (hadolint/docker if present)",
    },
    CatalogPlugin {
        name: "web",
        krate: "haw-web",
        description: "count/validate HTML & flag CSS assets per repo (stdlib only)",
    },
    CatalogPlugin {
        name: "k8s",
        krate: "haw-k8s",
        description: "find & client-side-validate Kubernetes manifests per repo",
    },
];

/// The first-party plugin source used by `haw plugins install` when no
/// `--git <url>` is given (these crates are workspace members, not on crates.io).
const PLUGIN_GIT_SOURCE: &str = "https://github.com/Nastwinns/hawser";

/// The default community index URL for `haw plugins list --remote`. It serves a
/// `haw.plugins.index/1` document — the repo-root `plugins-index.json`.
const DEFAULT_INDEX_URL: &str =
    "https://raw.githubusercontent.com/Nastwinns/hawser/main/plugins-index.json";

/// One plugin entry parsed from a `haw.plugins.index/1` community index.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteEntry {
    name: String,
    krate: Option<String>,
    git: Option<String>,
    description: String,
}

/// Parse a `haw.plugins.index/1` document into remote entries. Pure (no
/// network) so it is unit-testable against a canned index. Entries missing a
/// `name`, or whole documents with the wrong `schema`, are skipped; a malformed
/// document yields an error the caller can downgrade to a warning.
fn parse_index(json: &str) -> Result<Vec<RemoteEntry>> {
    let doc: serde_json::Value = serde_json::from_str(json).context("index is not valid JSON")?;
    if doc.get("schema").and_then(|s| s.as_str()) != Some("haw.plugins.index/1") {
        bail!("index is not a haw.plugins.index/1 document");
    }
    let mut out = Vec::new();
    if let Some(plugins) = doc.get("plugins").and_then(|p| p.as_array()) {
        for p in plugins {
            let Some(name) = p.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            out.push(RemoteEntry {
                name: name.to_string(),
                krate: p.get("crate").and_then(|c| c.as_str()).map(str::to_string),
                git: p.get("git").and_then(|g| g.as_str()).map(str::to_string),
                description: p
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    Ok(out)
}

/// Maximum body we will read from a remote index/manifest fetch (16 MiB). A
/// hostile server must not be able to stream unbounded data at us.
const FETCH_BODY_CAP: u64 = 16 * 1024 * 1024;

/// Extract the host component from an `http(s)://` URL without pulling in a URL
/// crate: strip the scheme, take everything before the first `/?#`, then drop
/// any `user@` and `:port` suffix. Returns `None` for non-http(s) inputs.
fn url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    // Drop userinfo (`user:pass@host`).
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    // Strip a trailing `:port`, honoring `[ipv6]:port` bracket form.
    let host = if let Some(end) = host_port.strip_prefix('[') {
        // `[::1]:443` -> `::1`
        end.split(']').next().unwrap_or(end).to_string()
    } else {
        host_port
            .rsplit_once(':')
            .map_or(host_port, |(h, _)| h)
            .to_string()
    };
    if host.is_empty() { None } else { Some(host) }
}

/// True when `ip` is a loopback, link-local, or RFC1918/unique-local private
/// address — i.e. an SSRF target we refuse to fetch from without `--insecure`.
fn is_internal_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()        // 127.0.0.0/8
                || v4.is_private()  // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local() // 169.254.0.0/16
                || v4.is_unspecified() // 0.0.0.0
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()        // ::1
                || v6.is_unspecified() // ::
                // fc00::/7 unique-local
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 link-local
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Validate a URL before we fetch it: enforce https-only and block internal
/// (loopback/link-local/private) literal-IP hosts. `--insecure` overrides both.
///
/// DNS-rebinding (a hostname that later resolves to an internal IP) is out of
/// scope; blocking literal-IP internal targets is the pragmatic hardening.
fn check_fetch_url(url: &str, insecure: bool) -> Result<()> {
    if insecure {
        return Ok(());
    }
    if !url.starts_with("https://") {
        if url.starts_with("http://") {
            bail!("refusing to fetch insecure http:// URL {url} (pass --insecure to allow)");
        }
        bail!(
            "refusing to fetch non-https URL {url} (only https:// is allowed; pass --insecure to override)"
        );
    }
    if let Some(host) = url_host(url)
        && let Ok(ip) = host.parse::<std::net::IpAddr>()
        && is_internal_ip(ip)
    {
        bail!(
            "refusing to fetch internal/private address {host} in {url} (pass --insecure to override)"
        );
    }
    Ok(())
}

/// A blocking HTTP client hardened for fetching untrusted index/manifest docs:
/// a bounded redirect policy and a request timeout.
fn hardened_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(3))
        .timeout(Duration::from_secs(30))
        .build()
        .context("building HTTP client")
}

/// Fetch `url` with the hardened client and read at most [`FETCH_BODY_CAP`]
/// bytes of the body (rejecting an over-cap `Content-Length` up front).
fn fetch_text_capped(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    use std::io::Read;
    let resp = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .with_context(|| format!("fetching {url}"))?;
    if let Some(len) = resp.content_length()
        && len > FETCH_BODY_CAP
    {
        bail!("{url}: response body {len} bytes exceeds cap of {FETCH_BODY_CAP} bytes");
    }
    let mut buf = Vec::new();
    resp.take(FETCH_BODY_CAP)
        .read_to_end(&mut buf)
        .with_context(|| format!("reading body of {url}"))?;
    String::from_utf8(buf).with_context(|| format!("{url}: response body is not UTF-8"))
}

/// Fetch and parse the community index at `url` (blocking). Errors here are
/// meant to be downgraded to a warning by the caller — the index is optional.
fn fetch_index(url: &str, insecure: bool) -> Result<Vec<RemoteEntry>> {
    check_fetch_url(url, insecure)?;
    let client = hardened_client()?;
    let text = fetch_text_capped(&client, url)?;
    parse_index(&text)
}

/// One merged plugin row for `haw plugins list`, deduped by name across the
/// catalog, PATH-discovered binaries, and manifest subscriptions.
struct PluginRow {
    name: String,
    krate: Option<String>,
    installed: bool,
    subscribed_phases: Vec<String>,
    description: String,
    /// `catalog`, `path`, or `subscribed` — where the row first came from.
    source: &'static str,
}

/// Merge the three plugin sources into a sorted, deduped-by-name row set.
/// `installed_names` are the bare `haw-<name>` names found on PATH;
/// `subscriptions` are `(name, phases)` from the manifest `[plugins]` map.
/// Factored out so it is testable without touching the real PATH or a workspace.
fn plugin_rows<'a, I>(installed_names: &[String], subscriptions: I) -> Vec<PluginRow>
where
    I: IntoIterator<Item = (&'a String, &'a Vec<String>)>,
{
    use std::collections::BTreeMap;

    let subs: HashMap<String, Vec<String>> = subscriptions
        .into_iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let installed: std::collections::HashSet<&str> =
        installed_names.iter().map(String::as_str).collect();

    // BTreeMap yields a stable, sorted, deduped-by-name result.
    let mut by_name: BTreeMap<String, PluginRow> = BTreeMap::new();

    for entry in PLUGIN_CATALOG {
        by_name.insert(
            entry.name.to_string(),
            PluginRow {
                name: entry.name.to_string(),
                krate: Some(entry.krate.to_string()),
                installed: installed.contains(entry.name),
                subscribed_phases: subs.get(entry.name).cloned().unwrap_or_default(),
                description: entry.description.to_string(),
                source: "catalog",
            },
        );
    }
    // PATH-discovered plugins not in the catalog still surface (source "path").
    for name in installed_names {
        by_name.entry(name.clone()).or_insert_with(|| PluginRow {
            name: name.clone(),
            krate: None,
            installed: true,
            subscribed_phases: subs.get(name).cloned().unwrap_or_default(),
            description: String::new(),
            source: "path",
        });
    }
    // Manifest subscriptions not otherwise known (source "subscribed").
    for (name, phases) in &subs {
        by_name.entry(name.clone()).or_insert_with(|| PluginRow {
            name: name.clone(),
            krate: None,
            installed: installed.contains(name.as_str()),
            subscribed_phases: phases.clone(),
            description: String::new(),
            source: "subscribed",
        });
    }
    by_name.into_values().collect()
}

/// Merge community-index entries into an existing (catalog/PATH/subscribed) row
/// set. A remote-only plugin appears as a new row with source `remote`, status
/// `available`, and the index's description. When a plugin already exists
/// (installed/catalog/subscribed) that row wins on status/source; the remote
/// entry only backfills an empty description. Dedup is by name; the result is
/// re-sorted. Pure so the merge is testable without network.
fn merge_remote(mut rows: Vec<PluginRow>, remote: &[RemoteEntry]) -> Vec<PluginRow> {
    use std::collections::HashSet;
    let known: HashSet<String> = rows.iter().map(|r| r.name.clone()).collect();
    for entry in remote {
        if known.contains(&entry.name) {
            // Existing rows keep their status/source; just backfill a description.
            if let Some(row) = rows.iter_mut().find(|r| r.name == entry.name)
                && row.description.is_empty()
            {
                row.description = entry.description.clone();
            }
            continue;
        }
        rows.push(PluginRow {
            name: entry.name.clone(),
            krate: entry.krate.clone(),
            installed: false,
            subscribed_phases: Vec::new(),
            description: entry.description.clone(),
            source: "remote",
        });
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

fn plugins_list(format: &str, remote: bool, index: Option<&str>, insecure: bool) -> Result<()> {
    if !matches!(format, "text" | "json") {
        bail!("unknown format `{format}` (use text or json)");
    }
    // A workspace is optional: subscriptions merge in when one is present.
    let subs: Vec<(String, Vec<String>)> = open_workspace()
        .map(|ws| {
            ws.manifest
                .plugins
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    let installed = plugins_on_path();
    let mut rows = plugin_rows(&installed, subs.iter().map(|(k, v)| (k, v)));

    // Optionally merge the community index. A network/parse failure warns and
    // falls back to local-only — never a hard failure.
    if remote {
        let url = index.unwrap_or(DEFAULT_INDEX_URL);
        match fetch_index(url, insecure) {
            Ok(entries) => rows = merge_remote(rows, &entries),
            Err(err) => {
                let c = Palette::new();
                eprintln!(
                    "{} community index unavailable ({err:#}) — showing local plugins only",
                    c.warn("warning:")
                );
            }
        }
    }

    if format == "json" {
        let plugins = rows
            .iter()
            .map(|r| {
                json!({
                    "name": r.name,
                    "crate": r.krate,
                    "installed": r.installed,
                    "subscribed_phases": r.subscribed_phases,
                    "description": r.description,
                    "source": r.source,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": "haw.plugins/1",
                "plugins": plugins,
            }))?
        );
        return Ok(());
    }

    let c = Palette::new();
    let width = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    println!(
        "{}",
        c.header(&format!(
            "{:<width$}  {:<10} {:<20} DESCRIPTION",
            "NAME", "STATUS", "SUBSCRIBED"
        ))
    );
    for r in &rows {
        let status = if r.installed {
            c.ok(&format!("{:<10}", "installed"))
        } else {
            c.dim(&format!("{:<10}", "available"))
        };
        let subscribed = if r.subscribed_phases.is_empty() {
            "-".to_string()
        } else {
            r.subscribed_phases.join(",")
        };
        println!(
            "{}  {status} {:<20} {}",
            c.name(&format!("{:<width$}", r.name)),
            c.rev(&subscribed),
            c.dim(&r.description),
        );
    }
    Ok(())
}

/// Resolve a plugin `name` to the crate to install: a catalog name maps to its
/// crate (`haw-aspice`); anything else is used verbatim so a full crate name
/// (e.g. `haw-foo` or a crates.io crate) still works.
fn resolve_install_crate(name: &str) -> String {
    PLUGIN_CATALOG
        .iter()
        .find(|e| e.name == name)
        .map(|e| e.krate.to_string())
        .unwrap_or_else(|| name.to_string())
}

/// How to pin a `cargo install --git` source to an immutable ref.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GitPin {
    /// `--tag <v>`.
    Tag(String),
    /// `--rev <sha>`.
    Rev(String),
}

/// Build the `cargo install` argument vector for `haw plugins install`.
///
/// SECURITY / supply chain: installs are ALWAYS `--locked` (build the exact
/// dependency versions the crate's `Cargo.lock` pins). The git source is ALWAYS
/// pinned to an immutable ref rather than tracking a moving branch:
/// - default first-party source -> pinned to this hawser release
///   (`--tag v<CARGO_PKG_VERSION>`),
/// - an explicit `--git` source -> pinned to the user-supplied `--tag`/`--rev`
///   when given (otherwise left to cargo's default, which the caller is opting
///   into by pointing at their own repo).
///
/// Factored out so the printed command is testable without running cargo.
fn cargo_install_args(name: &str, git: Option<&str>, pin: Option<&GitPin>) -> Vec<String> {
    let krate = resolve_install_crate(name);
    let source = git.unwrap_or(PLUGIN_GIT_SOURCE);
    let mut args: Vec<String> = vec![
        "install".to_string(),
        "--locked".to_string(),
        "--git".to_string(),
        source.to_string(),
    ];
    // Pin the ref: caller-supplied pin wins; otherwise the default first-party
    // source is pinned to this release's tag.
    match pin {
        Some(GitPin::Tag(t)) => {
            args.push("--tag".to_string());
            args.push(t.clone());
        }
        Some(GitPin::Rev(r)) => {
            args.push("--rev".to_string());
            args.push(r.clone());
        }
        None if git.is_none() => {
            args.push("--tag".to_string());
            args.push(format!("v{}", env!("CARGO_PKG_VERSION")));
        }
        None => {}
    }
    args.push(krate);
    args
}

fn plugins_install(
    name: &str,
    git: Option<&str>,
    pin: Option<GitPin>,
    dry_run: bool,
) -> Result<ExitCode> {
    let args = cargo_install_args(name, git, pin.as_ref());

    let c = Palette::new();
    // Print exactly what will run before running it.
    println!("{} cargo {}", c.dim("$"), args.join(" "));

    if dry_run {
        return Ok(ExitCode::SUCCESS);
    }

    let status = std::process::Command::new("cargo")
        .args(&args)
        .status()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!(
                    "`cargo` not found on PATH — install Rust from https://rustup.rs \
                     to use `haw plugins install`"
                )
            } else {
                anyhow::Error::from(err).context("failed to launch `cargo install`")
            }
        })?;
    Ok(ExitCode::from(
        status.code().unwrap_or(1).clamp(0, 255) as u8
    ))
}

fn plugins_path() {
    let c = Palette::new();
    let dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if dirs.is_empty() {
        println!("PATH is empty — no directories are scanned for `haw-*` plugins");
        return;
    }
    for dir in dirs {
        println!("{}", c.dim(&dir.display().to_string()));
    }
}

/// One file the scaffolder writes: a relative path, its contents, and whether
/// it must be marked executable (the plugin entry script/binary).
struct ScaffoldFile {
    path: &'static str,
    contents: String,
    executable: bool,
}

/// Build the set of files for a `haw plugins new <name> --lang <lang>` skeleton.
/// Pure (no filesystem) so the produced files/contents are unit-testable.
fn scaffold_files(name: &str, lang: PluginLang) -> Vec<ScaffoldFile> {
    let bin = format!("haw-{name}");
    match lang {
        PluginLang::Shell => vec![
            ScaffoldFile {
                path: "haw-NAME",
                contents: shell_skeleton(name),
                executable: true,
            },
            ScaffoldFile {
                path: "README.md",
                contents: readme_skeleton(name, &format!("./{bin}"), "shell"),
                executable: false,
            },
        ],
        PluginLang::Python => vec![
            ScaffoldFile {
                path: "haw-NAME",
                contents: python_skeleton(name),
                executable: true,
            },
            ScaffoldFile {
                path: "README.md",
                contents: readme_skeleton(name, &format!("./{bin}"), "python"),
                executable: false,
            },
        ],
        PluginLang::Go => vec![
            ScaffoldFile {
                path: "main.go",
                contents: go_skeleton(name),
                executable: false,
            },
            ScaffoldFile {
                path: "go.mod",
                contents: format!("module {bin}\n\ngo 1.21\n"),
                executable: false,
            },
            ScaffoldFile {
                path: "README.md",
                contents: readme_skeleton(name, &format!("go build -o {bin} && ./{bin}"), "go"),
                executable: false,
            },
        ],
        PluginLang::Rust => vec![
            ScaffoldFile {
                path: "Cargo.toml",
                contents: rust_cargo_toml(name),
                executable: false,
            },
            ScaffoldFile {
                path: "src/main.rs",
                contents: rust_skeleton(name),
                executable: false,
            },
            ScaffoldFile {
                path: "README.md",
                contents: readme_skeleton(
                    name,
                    &format!("cargo build --release   # target/release/{bin}"),
                    "rust",
                ),
                executable: false,
            },
        ],
    }
}

fn shell_skeleton(name: &str) -> String {
    format!(
        r##"#!/usr/bin/env sh
# haw-{name} — a haw plugin. Reads the haw.plugin/1 context from $HAW_JSON
# (falling back to stdin) and emits a haw.plugin.report/1 document.
set -eu

case "${{1:-}}" in
-h | --help)
	echo "haw-{name} — a haw plugin. Options: --help, --format json"
	echo "Run as: haw {name}"
	exit 0
	;;
esac

# haw hands us the workspace context in $HAW_JSON (and on stdin). Fall back to
# stdin when the env var is absent; degrade to empty when neither is present.
ctx="${{HAW_JSON:-}}"
if [ -z "$ctx" ] && [ ! -t 0 ]; then
	ctx=$(cat)
fi

# Best-effort extraction (no jq dependency): pull "root" out of the JSON.
root=$(printf '%s' "$ctx" | sed -n 's/.*"root":"\([^"]*\)".*/\1/p')

if [ "${{1:-}}" = "--format" ] && [ "${{2:-}}" = "json" ]; then
	printf '{{"schema":"haw.plugin.report/1","ok":true,"plugin":"{name}","summary":"haw-{name} ran","root":"%s"}}\n' "$root"
	exit 0
fi

if [ -n "$root" ]; then
	printf 'haw-{name}: workspace at %s\n' "$root"
else
	printf 'haw-{name}: no workspace here — operating on the current directory\n'
fi
"##
    )
}

fn python_skeleton(name: &str) -> String {
    format!(
        r#"#!/usr/bin/env python3
# haw-{name} — a haw plugin. Reads the haw.plugin/1 context from HAW_JSON
# (falling back to stdin) and emits a haw.plugin.report/1 document.
import json
import os
import sys


def load_context():
    raw = os.environ.get("HAW_JSON", "")
    if not raw and not sys.stdin.isatty():
        raw = sys.stdin.read()
    if not raw:
        return {{}}
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {{}}


def main() -> int:
    args = sys.argv[1:]
    if "-h" in args or "--help" in args:
        print("haw-{name} — a haw plugin. Options: --help, --format json")
        print("Run as: haw {name}")
        return 0

    ctx = load_context()
    root = ctx.get("root")
    repos = ctx.get("repos", []) or []

    if args[:2] == ["--format", "json"]:
        report = {{
            "schema": "haw.plugin.report/1",
            "ok": True,
            "plugin": "{name}",
            "summary": "haw-{name} inspected {{}} repo(s)".format(len(repos)),
            "root": root,
        }}
        json.dump(report, sys.stdout)
        sys.stdout.write("\n")
        return 0

    if root:
        print("haw-{name}: workspace at {{}} ({{}} repos)".format(root, len(repos)))
    else:
        print("haw-{name}: no workspace here — operating on the current directory")
    return 0


if __name__ == "__main__":
    sys.exit(main())
"#
    )
}

fn go_skeleton(name: &str) -> String {
    format!(
        r#"// haw-{name} — a haw plugin. Reads the haw.plugin/1 context from HAW_JSON
// (falling back to stdin) and emits a haw.plugin.report/1 document.
//
// Build:  go build -o haw-{name}
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
)

type context struct {{
	Schema string `json:"schema"`
	Root   string `json:"root"`
	Repos  []struct {{
		Name string `json:"name"`
	}} `json:"repos"`
}}

type report struct {{
	Schema  string `json:"schema"`
	OK      bool   `json:"ok"`
	Plugin  string `json:"plugin"`
	Summary string `json:"summary"`
	Root    string `json:"root,omitempty"`
}}

func loadContext() context {{
	var ctx context
	raw := os.Getenv("HAW_JSON")
	if raw == "" {{
		if stat, err := os.Stdin.Stat(); err == nil && (stat.Mode()&os.ModeCharDevice) == 0 {{
			if b, err := io.ReadAll(os.Stdin); err == nil {{
				raw = string(b)
			}}
		}}
	}}
	if raw != "" {{
		_ = json.Unmarshal([]byte(raw), &ctx)
	}}
	return ctx
}}

func main() {{
	args := os.Args[1:]
	for _, a := range args {{
		if a == "-h" || a == "--help" {{
			fmt.Println("haw-{name} — a haw plugin. Options: --help, --format json")
			fmt.Println("Run as: haw {name}")
			return
		}}
	}}

	ctx := loadContext()

	if len(args) >= 2 && args[0] == "--format" && args[1] == "json" {{
		rep := report{{
			Schema:  "haw.plugin.report/1",
			OK:      true,
			Plugin:  "{name}",
			Summary: fmt.Sprintf("haw-{name} inspected %d repo(s)", len(ctx.Repos)),
			Root:    ctx.Root,
		}}
		out, _ := json.Marshal(rep)
		fmt.Println(string(out))
		return
	}}

	if ctx.Root != "" {{
		fmt.Printf("haw-{name}: workspace at %s (%d repos)\n", ctx.Root, len(ctx.Repos))
	}} else {{
		fmt.Println("haw-{name}: no workspace here — operating on the current directory")
	}}
}}
"#
    )
}

fn rust_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "haw-{name}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "haw-{name}"
path = "src/main.rs"

[dependencies]
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#
    )
}

fn rust_skeleton(name: &str) -> String {
    format!(
        r##"// haw-{name} — a haw plugin. Reads the haw.plugin/1 context from HAW_JSON
// (falling back to stdin) and emits a haw.plugin.report/1 document.
// Standalone: depends only on serde/serde_json, not on any haw crate.
use std::env;
use std::io::{{IsTerminal, Read}};
use std::process::ExitCode;

use serde::{{Deserialize, Serialize}};

#[derive(Default, Deserialize)]
struct Context {{
    #[serde(default)]
    root: Option<String>,
    #[serde(default)]
    repos: Vec<Repo>,
}}

#[derive(Deserialize)]
struct Repo {{
    #[allow(dead_code)]
    name: String,
}}

#[derive(Serialize)]
struct Report {{
    schema: &'static str,
    ok: bool,
    plugin: &'static str,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    root: Option<String>,
}}

/// Read the context from HAW_JSON, falling back to stdin, degrading to empty.
fn load_context() -> Context {{
    let mut raw = env::var("HAW_JSON").unwrap_or_default();
    if raw.is_empty() && !std::io::stdin().is_terminal() {{
        let _ = std::io::stdin().read_to_string(&mut raw);
    }}
    if raw.trim().is_empty() {{
        return Context::default();
    }}
    serde_json::from_str(&raw).unwrap_or_default()
}}

fn main() -> ExitCode {{
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {{
        println!("haw-{name} — a haw plugin. Options: --help, --format json");
        println!("Run as: haw {name}");
        return ExitCode::SUCCESS;
    }}

    let ctx = load_context();

    if args == ["--format", "json"] {{
        let report = Report {{
            schema: "haw.plugin.report/1",
            ok: true,
            plugin: "{name}",
            summary: format!("haw-{name} inspected {{}} repo(s)", ctx.repos.len()),
            root: ctx.root.clone(),
        }};
        match serde_json::to_string(&report) {{
            Ok(json) => println!("{{json}}"),
            Err(err) => {{
                eprintln!("haw-{name}: failed to serialize report: {{err}}");
                return ExitCode::FAILURE;
            }}
        }}
        return ExitCode::SUCCESS;
    }}

    match ctx.root {{
        Some(root) => println!(
            "haw-{name}: workspace at {{root}} ({{}} repos)",
            ctx.repos.len()
        ),
        None => println!("haw-{name}: no workspace here — operating on the current directory"),
    }}
    ExitCode::SUCCESS
}}
"##
    )
}

fn readme_skeleton(name: &str, build: &str, lang: &str) -> String {
    let bin = format!("haw-{name}");
    format!(
        r#"# {bin}

A [haw](https://github.com/Nastwinns/hawser) plugin ({lang}). Any executable named
`haw-<name>` on your `PATH` becomes `haw <name>`.

## Contract

- Reads the `haw.plugin/1` context from the `HAW_JSON` environment variable
  (falling back to stdin) — degrades gracefully when run outside a workspace.
- Handles `--help` and `--format json`.
- Emits a `haw.plugin.report/1` document under `--format json`.

## Build & install

```sh
{build}
```

Drop the resulting `{bin}` executable onto your `PATH`, then:

```sh
which {bin}          # haw finds exactly what your shell finds
haw {name}           # dispatched to {bin}
haw {name} --format json
```

## Subscribe to a lifecycle phase (optional)

Add it to your workspace manifest `[plugins]` table to run it on a phase:

```toml
[plugins]
{name} = ["post-build"]   # e.g. pre-sync, post-sync, pre-request, post-land
```
"#
    )
}

/// Scaffold a runnable plugin skeleton in a new directory. Refuses to overwrite
/// a non-empty target; prints the files created and the next steps.
fn plugins_new(name: &str, lang: PluginLang, dir: Option<&Path>) -> Result<()> {
    if name.is_empty() {
        bail!("plugin name must not be empty");
    }
    let bin = format!("haw-{name}");
    let target = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_dir()?.join(&bin),
    };

    // Refuse to clobber an existing non-empty directory.
    if target.exists() {
        let non_empty = std::fs::read_dir(&target)
            .map(|mut it| it.next().is_some())
            .unwrap_or(false);
        if non_empty {
            bail!(
                "target directory {} is not empty — choose an empty dir with --dir <path>",
                target.display()
            );
        }
    }
    std::fs::create_dir_all(&target)?;

    let files = scaffold_files(name, lang);
    let c = Palette::new();
    let mut written = Vec::new();
    for file in &files {
        // `haw-NAME` is a placeholder for the real binary name.
        let rel = if file.path == "haw-NAME" {
            bin.clone()
        } else {
            file.path.to_string()
        };
        let dest = target.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &file.contents)?;
        if file.executable {
            make_executable(&dest)?;
        }
        written.push(rel);
    }

    println!(
        "{}",
        c.bold(&format!(
            "scaffolded {bin} ({}) in {}",
            lang_label(lang),
            target.display()
        ))
    );
    for rel in &written {
        println!("  {} {}", c.ok("+"), c.dim(rel));
    }
    println!();
    println!("next:");
    match lang {
        PluginLang::Shell | PluginLang::Python => {
            println!("  PATH=\"{}:$PATH\" haw {name}", target.display());
        }
        PluginLang::Go => {
            println!("  (cd {} && go build -o {bin})", target.display());
            println!("  PATH=\"{}:$PATH\" haw {name}", target.display());
        }
        PluginLang::Rust => {
            println!("  (cd {} && cargo build --release)", target.display());
            println!(
                "  PATH=\"{}/target/release:$PATH\" haw {name}",
                target.display()
            );
        }
    }
    Ok(())
}

fn lang_label(lang: PluginLang) -> &'static str {
    match lang {
        PluginLang::Rust => "rust",
        PluginLang::Python => "python",
        PluginLang::Go => "go",
        PluginLang::Shell => "shell",
    }
}

/// Mark a scaffolded entry script/binary executable. A no-op on non-Unix.
fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn plugin(args: &[String]) -> Result<ExitCode> {
    let Some((name, rest)) = args.split_first() else {
        bail!("empty plugin invocation");
    };
    let binary = format!("haw-{name}");
    let context = match open_workspace() {
        Ok(ws) => json!({
            "schema": "haw.plugin/1",
            "root": ws.root.to_string_lossy(),
            "stack": ws.current_stack(),
            "repos": ws.manifest.repos.iter().map(|(repo_name, repo)| json!({
                "name": repo_name,
                "path": ws.root.join(repo.checkout_path(repo_name)).to_string_lossy(),
                "rev": repo.rev,
                "groups": repo.groups,
            })).collect::<Vec<_>>(),
        }),
        Err(_) => json!({"schema": "haw.plugin/1"}),
    };

    use std::io::Write;
    let mut child = std::process::Command::new(&binary)
        .args(rest)
        .env("HAW_JSON", context.to_string())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("no built-in `{name}` and no `{binary}` on PATH"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(context.to_string().as_bytes());
    }
    let status = child.wait()?;
    Ok(ExitCode::from(
        status.code().unwrap_or(1).clamp(0, 255) as u8
    ))
}

/// Discover the available plugin panels: the manifest `[plugins]` keys unioned
/// with every `haw-*` executable found on `PATH`, deduped and sorted. Manifest
/// entries carry their subscribed phases; PATH-only discoveries have none.
pub(crate) fn discover_plugin_panels<'a, I>(subscriptions: I) -> Vec<haw_tui::PluginPanel>
where
    I: IntoIterator<Item = (&'a String, &'a Vec<String>)>,
{
    // Show the official first-party catalog too (mirrors `haw plugins list`), so the
    // Plugins view (`P`) is never empty just because nothing is installed/subscribed
    // yet — deduped by name, subscriptions/PATH still win for the phases.
    let mut names = plugins_on_path();
    names.extend(PLUGIN_CATALOG.iter().map(|e| e.name.to_string()));
    merge_plugin_panels(subscriptions, names)
}

/// Merge manifest subscriptions with a set of PATH-discovered plugin names into
/// a sorted, deduped panel list. Factored out of [`discover_plugin_panels`] so
/// the merge is testable without touching `PATH`.
fn merge_plugin_panels<'a, I>(
    subscriptions: I,
    path_names: Vec<String>,
) -> Vec<haw_tui::PluginPanel>
where
    I: IntoIterator<Item = (&'a String, &'a Vec<String>)>,
{
    use std::collections::BTreeMap;

    // BTreeMap keeps a stable, sorted, deduped-by-name result.
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, phases) in subscriptions {
        by_name.insert(name.clone(), phases.clone());
    }
    for name in path_names {
        by_name.entry(name).or_default();
    }
    by_name
        .into_iter()
        .map(|(name, phases)| haw_tui::PluginPanel { name, phases })
        .collect()
}

/// Bare names of every `haw-<name>` executable found across the directories on
/// `PATH` (best-effort; unreadable dirs are skipped).
fn plugins_on_path() -> Vec<String> {
    match std::env::var_os("PATH") {
        Some(path) => plugins_in_dirs(std::env::split_paths(&path)),
        None => Vec::new(),
    }
}

/// Bare `haw-<name>` executable names across `dirs` (unreadable dirs skipped).
/// Windows executable extensions are stripped so `haw-sbom.exe` surfaces as
/// `sbom`. Factored out of [`plugins_on_path`] so it is testable without
/// mutating the process environment.
fn plugins_in_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Vec<String> {
    let mut names = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let raw = file_name.to_string_lossy();
            let stem = raw
                .strip_suffix(".exe")
                .or_else(|| raw.strip_suffix(".bat"))
                .or_else(|| raw.strip_suffix(".cmd"))
                .unwrap_or(&raw);
            if let Some(name) = stem.strip_prefix("haw-")
                && !name.is_empty()
            {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// Run `haw-<name>` in a render intent and return the text panel for the TUI.
///
/// The render contract: the normal `haw.plugin/1` context plus `"intent":
/// "render"` on stdin and `HAW_JSON`, and `HAW_RENDER=1` in the environment. If
/// the plugin emits a `haw.plugin.view/1` document (`{title, lines}`) its title
/// and lines are rendered; otherwise the raw stdout is shown. Output is
/// line-capped to keep the detail view bounded.
/// A friendly detail-body panel shown when a catalog plugin isn't installed.
/// Returned as `Ok` (not an error) so the TUI renders it without logging.
fn not_installed_hint(name: &str) -> String {
    let first_party = PLUGIN_CATALOG.iter().any(|e| e.name == name);
    let mut hint = format!(
        "Plugin \"{name}\" is not installed.\n\nInstall it with:\n  haw plugins install {name}\n"
    );
    if first_party {
        hint.push_str("\n(It's a first-party plugin from the catalog.)\n");
    }
    hint
}

pub(crate) fn render_plugin_panel(ws: &Workspace, name: &str) -> std::io::Result<String> {
    use std::io::Write;

    let binary = format!("haw-{name}");

    // A catalog plugin that simply isn't installed is an EXPECTED state, not an
    // error: render a friendly install hint (as Ok) so the TUI shows it in the
    // detail body without logging an error.
    if !plugins_on_path().iter().any(|n| n == name) {
        return Ok(not_installed_hint(name));
    }

    let context = json!({
        "schema": "haw.plugin/1",
        "intent": "render",
        "root": ws.root.to_string_lossy(),
        "stack": ws.current_stack(),
        "repos": ws.manifest.repos.iter().map(|(repo_name, repo)| json!({
            "name": repo_name,
            "path": ws.root.join(repo.checkout_path(repo_name)).to_string_lossy(),
            "rev": repo.rev,
            "groups": repo.groups,
        })).collect::<Vec<_>>(),
    });
    let body = context.to_string();

    let spawned = std::process::Command::new(&binary)
        .env("HAW_JSON", &body)
        .env("HAW_RENDER", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            // Raced with an uninstall between listing and render: still a
            // not-installed state, so render the hint rather than an error.
            return Ok(not_installed_hint(name));
        }
        Err(err) => return Err(err),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(body.as_bytes());
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(haw_forge::cap_lines(&plugin_view_text(name, &stdout), 600))
}

/// Turn a plugin's render stdout into panel text. A `haw.plugin.view/1`
/// document (`{schema, title, lines}`) renders as its title followed by its
/// lines; anything else falls back to the raw stdout (or an empty-output note).
fn plugin_view_text(name: &str, stdout: &str) -> String {
    if let Ok(view) = serde_json::from_str::<serde_json::Value>(stdout.trim())
        && view.get("schema").and_then(|s| s.as_str()) == Some("haw.plugin.view/1")
    {
        let title = view
            .get("title")
            .and_then(|t| t.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("plugin: {name}"));
        let mut out = String::new();
        out.push_str(&title);
        out.push('\n');
        if let Some(lines) = view.get("lines").and_then(|l| l.as_array()) {
            for line in lines {
                if let Some(text) = line.as_str() {
                    out.push_str(text);
                }
                out.push('\n');
            }
        }
        return out;
    }
    if stdout.trim().is_empty() {
        format!("plugin `{name}` produced no output\n")
    } else {
        stdout.to_string()
    }
}

fn import_manifest(from: &Path) -> Result<()> {
    let dest = PathBuf::from(MANIFEST_FILE);
    if dest.exists() {
        bail!("{MANIFEST_FILE} already exists here");
    }
    let manifest = import::import(from)?;
    let text = toml::to_string_pretty(&manifest)?;
    std::fs::write(&dest, text)?;
    println!(
        "imported {} repo(s) from {} into {MANIFEST_FILE}",
        manifest.repos.len(),
        from.display()
    );
    println!(
        "one stack `{}` holds every repo — split it into real stacks as needed",
        import::DEFAULT_STACK
    );
    println!("next: haw lock && haw sync");
    Ok(())
}

/// Read cap for local file content (~1 MB).
pub(crate) const FILE_SIZE_CAP: u64 = 1_048_576;

/// Render already-read file bytes for the detail view: a binary NUL-sniff on
/// the first 8 KB yields a placeholder, else the (line-capped) text.
pub(crate) fn render_file_bytes(bytes: &[u8]) -> String {
    let sniff = &bytes[..bytes.len().min(8192)];
    if sniff.contains(&0) {
        return format!("<binary file, {} bytes>\n", bytes.len());
    }
    let text = String::from_utf8_lossy(bytes);
    haw_forge::cap_lines(&text, 600)
}

/// A repo's absolute checkout root, or an error when it isn't in the manifest.
pub(crate) fn repo_root(ws: &Workspace, repo: &str) -> std::io::Result<PathBuf> {
    let spec =
        ws.manifest.repos.get(repo).ok_or_else(|| {
            std::io::Error::other(format!("repo `{repo}` is not in the manifest"))
        })?;
    let root = ws.root.join(spec.checkout_path(repo));
    if !ShellGit.is_repo(&root) {
        return Err(std::io::Error::other(format!(
            "repo `{repo}` is not cloned at {}; press s to sync or R for the forge view",
            root.display()
        )));
    }
    Ok(root)
}

/// The repo's locked SHA from haw.lock, if a lock exists and lists it.
pub(crate) fn locked_sha(ws: &Workspace, repo: &str) -> Option<String> {
    ws.read_lock().ok().flatten().and_then(|lock| {
        lock.repos
            .iter()
            .find(|r| r.name == repo)
            .map(|r| r.rev.clone())
    })
}

/// Join `subpath` under `root` and refuse any path that escapes it (path
/// traversal). Canonicalizes both sides so `..`, symlinks, and `.` are all
/// resolved before the containment check.
pub(crate) fn safe_join(root: &Path, subpath: &str) -> std::io::Result<PathBuf> {
    let sub = subpath.trim_matches('/');
    let candidate = if sub.is_empty() {
        root.to_path_buf()
    } else {
        root.join(sub)
    };
    let real_root = root.canonicalize()?;
    let real = candidate.canonicalize()?;
    if !real.starts_with(&real_root) {
        return Err(std::io::Error::other(format!(
            "refusing path outside the repo: {subpath}"
        )));
    }
    Ok(real)
}

/// Resolve a repo's clone URL and a ready-to-call forge client, honoring the
/// manifest's explicit `forge =` key (mirrors `orchestrate::forge_hint`).
pub(crate) fn forge_for_repo(
    ws: &Workspace,
    repo: &str,
) -> std::io::Result<(Box<dyn haw_forge::Forge>, String)> {
    use haw_forge::ForgeFactory;
    let spec =
        ws.manifest.repos.get(repo).ok_or_else(|| {
            std::io::Error::other(format!("repo `{repo}` is not in the manifest"))
        })?;
    let url = spec.clone_url(&ws.manifest.remotes).ok_or_else(|| {
        std::io::Error::other(format!("repo `{repo}` has no resolvable clone URL"))
    })?;
    let hint = spec
        .remote
        .as_deref()
        .and_then(|name| ws.manifest.remotes.get(name))
        .and_then(|remote| remote.forge.as_deref())
        .and_then(haw_forge::kind_from_key);
    let tokens = Tokens::from_env();
    let forge = tokens
        .client_for(&url, hint)
        .map_err(std::io::Error::other)?;
    Ok((forge, url))
}

/// The cloned repos of `stack` (or the whole fleet when `stack` is `None`) as
/// `(name, absolute path)`, honoring haw.lock when present, else the manifest.
/// Skips repos that aren't cloned so cross-repo grep never errors on them.
pub(crate) fn fleet_repos(
    ws: &Workspace,
    stack: Option<&str>,
) -> std::io::Result<Vec<(String, PathBuf)>> {
    let backend = ShellGit;
    let allowed: Option<Vec<String>> = match stack {
        Some(name) => {
            let spec = ws.manifest.stacks.get(name).ok_or_else(|| {
                std::io::Error::other(format!("stack `{name}` is not in the manifest"))
            })?;
            Some(spec.repos.clone())
        }
        None => None,
    };
    let repos: Vec<(String, PathBuf)> = match ws.read_lock().map_err(std::io::Error::other)? {
        Some(lock) => lock
            .repos
            .iter()
            .map(|r| (r.name.clone(), ws.root.join(&r.path)))
            .collect(),
        None => ws
            .manifest
            .repos
            .iter()
            .map(|(name, repo)| (name.clone(), ws.root.join(repo.checkout_path(name))))
            .collect(),
    };
    Ok(repos
        .into_iter()
        .filter(|(name, _)| allowed.as_ref().is_none_or(|set| set.contains(name)))
        .filter(|(_, path)| backend.is_repo(path))
        .collect())
}

/// Run `git grep -n --no-color -e <pattern>` in `path`, returning stdout. Exit
/// code 1 is git-grep's "no match" — treated as empty, not an error.
pub(crate) fn git_grep(path: &Path, pattern: &str) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["grep", "-n", "--no-color", "-e", pattern])
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Err(_) => String::new(),
    }
}

/// Run `git -C <path> <args...>`, mapping a non-zero exit or spawn failure to
/// an `io::Error` carrying git's stderr. Used for write/exec git operations.
pub(crate) fn run_git(path: &Path, args: &[&str]) -> std::io::Result<()> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(std::io::Error::other(format!(
        "git {} failed: {}",
        args.join(" "),
        stderr.trim()
    )))
}

/// Run `git -C <path> <args...>`, returning trimmed stdout or an error note.
pub(crate) fn git_capture(path: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim_end().to_string()
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            format!("(git {}: {})", args.join(" "), err.trim())
        }
        Err(err) => format!("(git {}: {err})", args.join(" ")),
    }
}

/// Compose a readable, plain-text git report for one repo's checkout. Returns a
/// "not cloned" note (not an error) when the path holds no git repository.
pub(crate) fn git_detail_report(repo: &str, path: &Path) -> String {
    if !ShellGit.is_repo(path) {
        return format!(
            "== {repo} ==\n\nnot cloned at {}\n\nrun `haw sync` to clone it.",
            path.display()
        );
    }
    let sha = git_capture(path, &["rev-parse", "--short", "HEAD"]);
    let branch = git_capture(path, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let status = git_capture(path, &["status", "-sb"]);
    let log = git_capture(path, &["log", "--oneline", "--decorate", "-20"]);
    let show = git_capture(path, &["show", "--stat", "--oneline", "HEAD"]);
    let show: String = show.lines().take(40).collect::<Vec<_>>().join("\n");
    let remotes = git_capture(path, &["remote", "-v"]);

    let mut report = String::new();
    report.push_str(&format!("== {repo} ==\n"));
    report.push_str(&format!("branch {branch}  @ {sha}\n\n"));
    report.push_str("-- status --\n");
    report.push_str(&status);
    report.push_str("\n\n-- recent commits --\n");
    report.push_str(&log);
    report.push_str("\n\n-- last commit --\n");
    report.push_str(&show);
    report.push_str("\n\n-- remotes --\n");
    report.push_str(&remotes);
    report.push('\n');
    report
}

fn dash(demo: bool) -> Result<()> {
    let controller: Box<dyn haw_tui::Controller> = if demo {
        Box::new(DemoController)
    } else {
        open_workspace()?;
        Box::new(CliController::default())
    };
    // `haw_tui::run` restores the terminal before returning, so the TTY is
    // cooked by the time we act on the exit request.
    match haw_tui::run(controller)? {
        Some(haw_tui::Exit::Goto(path)) => println!("{}", path.display()),
        Some(haw_tui::Exit::Shell(path)) => launch_shell(&path)?,
        None => {}
    }
    Ok(())
}

/// Drop the user into an interactive shell rooted at `path`. When stdout is not
/// a terminal (scripted `cd "$(haw dash)"`), print the path instead so the
/// cockpit stays scriptable.
fn launch_shell(path: &Path) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        println!("{}", path.display());
        return Ok(());
    }
    #[cfg(windows)]
    let shell = std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd".into());
    #[cfg(not(windows))]
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    std::process::Command::new(&shell)
        .current_dir(path)
        .status()
        .with_context(|| {
            format!(
                "launching {} in {}",
                shell.to_string_lossy(),
                path.display()
            )
        })?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod plugin_panel_tests {
    use super::*;

    #[test]
    fn plugins_in_dirs_finds_haw_prefixed_only() {
        // A temp dir with haw-* executables plus a decoy without the prefix.
        let dir = tempfile::tempdir().unwrap();
        for name in ["haw-sbom", "haw-scan", "not-a-plugin"] {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        let mut names = plugins_in_dirs([dir.path().to_path_buf()]);
        names.sort();
        assert_eq!(names, vec!["sbom".to_string(), "scan".to_string()]);
    }

    #[test]
    fn merge_unions_manifest_and_path_and_dedups() {
        let subs: Vec<(String, Vec<String>)> = vec![
            ("sbom".to_string(), vec!["post-build".to_string()]),
            ("sign".to_string(), vec!["post-land".to_string()]),
        ];
        // `sbom` is in BOTH the manifest and on PATH — it must dedup.
        let path_names = vec!["sbom".to_string(), "scan".to_string()];

        let panels = merge_plugin_panels(subs.iter().map(|(k, v)| (k, v)), path_names);
        let names: Vec<&str> = panels.iter().map(|p| p.name.as_str()).collect();
        // Union of {sbom, sign} and {sbom, scan}, sorted and deduped.
        assert_eq!(names, vec!["sbom", "scan", "sign"]);
        // The manifest entry keeps its phases; the PATH-only entry has none.
        let sbom = panels.iter().find(|p| p.name == "sbom").unwrap();
        assert_eq!(sbom.phases, vec!["post-build".to_string()]);
        let scan = panels.iter().find(|p| p.name == "scan").unwrap();
        assert!(scan.phases.is_empty());
    }

    #[test]
    fn catalog_has_the_first_party_and_starter_plugins() {
        let names: Vec<&str> = PLUGIN_CATALOG.iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            vec![
                // First-party crates.
                "aspice",
                "jira",
                "misra",
                "compliance",
                "artifact",
                "git-gate",
                // Starter example plugins (examples/plugins/).
                "fleet-status",
                "docker",
                "web",
                "k8s",
            ]
        );
    }

    #[test]
    fn list_marks_catalog_plugin_installed_when_binary_on_path() {
        // Fabricate a PATH dir with a `haw-aspice` binary — do NOT touch real PATH.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("haw-aspice"), b"").unwrap();
        let installed = plugins_in_dirs([dir.path().to_path_buf()]);

        let no_subs: Vec<(String, Vec<String>)> = Vec::new();
        let rows = plugin_rows(&installed, no_subs.iter().map(|(k, v)| (k, v)));

        let aspice = rows.iter().find(|r| r.name == "aspice").unwrap();
        assert!(
            aspice.installed,
            "haw-aspice binary is on the fabricated PATH"
        );
        assert_eq!(aspice.source, "catalog");
        // A catalog plugin whose binary is absent is `available`, not installed.
        let jira = rows.iter().find(|r| r.name == "jira").unwrap();
        assert!(!jira.installed);
    }

    #[test]
    fn not_installed_hint_is_a_friendly_install_panel() {
        // A first-party catalog plugin gets the install command AND the catalog note.
        let hint = not_installed_hint("artifact");
        assert!(hint.contains("Plugin \"artifact\" is not installed."));
        assert!(hint.contains("haw plugins install artifact"));
        assert!(hint.contains("first-party plugin from the catalog"));
        // Not an error string — no "failed"/"nothing to render" language.
        assert!(!hint.contains("failed"));
        assert!(!hint.contains("nothing to render"));

        // A non-catalog plugin still gets the hint, minus the catalog note.
        let other = not_installed_hint("bespoke");
        assert!(other.contains("haw plugins install bespoke"));
        assert!(!other.contains("first-party"));
    }

    #[test]
    fn render_plugin_panel_returns_ok_hint_when_not_installed() {
        // A plugin whose `haw-<name>` binary is absent from PATH renders the
        // install hint as Ok — it must NOT surface as an Err (which the TUI logs).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MANIFEST_FILE);
        std::fs::write(
            &path,
            "[repo.a]\nurl = \"https://example.com/a.git\"\nrev = \"main\"\n",
        )
        .unwrap();
        let ws = Workspace::open_manifest(&path).unwrap();
        let out = render_plugin_panel(&ws, "definitely-not-installed-xyz")
            .expect("not-installed is Ok, not Err");
        assert!(out.contains("is not installed"));
        assert!(out.contains("haw plugins install definitely-not-installed-xyz"));
    }

    #[test]
    fn list_merges_subscription_phases() {
        let subs: Vec<(String, Vec<String>)> =
            vec![("misra".to_string(), vec!["pre-request".to_string()])];
        let installed: Vec<String> = Vec::new();
        let rows = plugin_rows(&installed, subs.iter().map(|(k, v)| (k, v)));
        let misra = rows.iter().find(|r| r.name == "misra").unwrap();
        assert_eq!(misra.subscribed_phases, vec!["pre-request".to_string()]);
    }

    #[test]
    fn install_crate_resolves_catalog_name_to_crate() {
        assert_eq!(resolve_install_crate("aspice"), "haw-aspice");
        // A verbatim crate name passes through unchanged.
        assert_eq!(resolve_install_crate("haw-custom"), "haw-custom");
    }

    #[test]
    fn install_dry_run_prints_expected_command_and_runs_nothing() {
        // The exact command `haw plugins install aspice --dry-run` prints: the
        // default first-party source is ALWAYS --locked and pinned to this
        // release's tag.
        let args = cargo_install_args("aspice", None, None);
        assert_eq!(
            format!("cargo {}", args.join(" ")),
            format!(
                "cargo install --locked --git https://github.com/Nastwinns/hawser --tag v{} haw-aspice",
                env!("CARGO_PKG_VERSION")
            )
        );
        // Supply-chain invariants.
        assert!(
            args.iter().any(|a| a == "--locked"),
            "installs must be --locked"
        );
        assert!(
            args.iter().any(|a| a == "--tag"),
            "default source must be pinned"
        );
        // --dry-run must not launch cargo; it returns SUCCESS after printing.
        let code = plugins_install("aspice", None, None, true).unwrap();
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::SUCCESS));
    }

    #[test]
    fn install_custom_git_is_locked_and_takes_pin_passthrough() {
        // Custom --git: still --locked; NOT auto-pinned to hawser's tag; honors
        // an explicit --tag/--rev passthrough.
        let args = cargo_install_args("haw-foo", Some("https://example.com/me/plugins"), None);
        assert_eq!(
            args,
            vec![
                "install",
                "--locked",
                "--git",
                "https://example.com/me/plugins",
                "haw-foo",
            ]
        );

        let tagged = cargo_install_args(
            "haw-foo",
            Some("https://example.com/me/plugins"),
            Some(&GitPin::Tag("v9.9.9".to_string())),
        );
        assert_eq!(
            tagged,
            vec![
                "install",
                "--locked",
                "--git",
                "https://example.com/me/plugins",
                "--tag",
                "v9.9.9",
                "haw-foo",
            ]
        );

        let rev = cargo_install_args(
            "haw-foo",
            Some("https://example.com/me/plugins"),
            Some(&GitPin::Rev("deadbeef".to_string())),
        );
        assert!(rev.iter().any(|a| a == "--locked"));
        assert_eq!(rev[rev.len() - 3..], ["--rev", "deadbeef", "haw-foo"]);
    }

    #[test]
    fn url_host_extracts_authority() {
        assert_eq!(
            url_host("https://example.com/a/b").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            url_host("https://user:pw@example.com:8443/x").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            url_host("http://127.0.0.1:80").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(url_host("https://[::1]:443/y").as_deref(), Some("::1"));
        assert_eq!(url_host("ftp://example.com"), None);
    }

    #[test]
    fn internal_ip_detection() {
        use std::net::IpAddr;
        for s in [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.9.9",
            "192.168.1.1",
            "169.254.1.1",
            "::1",
            "fc00::1",
            "fe80::1",
        ] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(is_internal_ip(ip), "{s} should be internal");
        }
        for s in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(!is_internal_ip(ip), "{s} should be public");
        }
    }

    #[test]
    fn check_fetch_url_rejects_http_and_internal_but_allows_https_public() {
        // https-only.
        assert!(check_fetch_url("http://example.com/i.json", false).is_err());
        assert!(check_fetch_url("file:///etc/passwd", false).is_err());
        // internal literal IPs blocked.
        assert!(check_fetch_url("https://127.0.0.1/i.json", false).is_err());
        assert!(check_fetch_url("https://10.1.2.3/i.json", false).is_err());
        assert!(check_fetch_url("https://[::1]/i.json", false).is_err());
        // public https allowed.
        assert!(check_fetch_url("https://example.com/i.json", false).is_ok());
        assert!(check_fetch_url("https://8.8.8.8/i.json", false).is_ok());
        // --insecure overrides everything.
        assert!(check_fetch_url("http://127.0.0.1/i.json", true).is_ok());
    }

    #[test]
    fn list_json_schema_and_a_known_catalog_name() {
        // Build rows the way `plugins_list` does, then assert the JSON shape.
        let installed: Vec<String> = Vec::new();
        let no_subs: Vec<(String, Vec<String>)> = Vec::new();
        let rows = plugin_rows(&installed, no_subs.iter().map(|(k, v)| (k, v)));
        let plugins = rows
            .iter()
            .map(|r| {
                json!({
                    "name": r.name,
                    "crate": r.krate,
                    "installed": r.installed,
                    "subscribed_phases": r.subscribed_phases,
                    "description": r.description,
                    "source": r.source,
                })
            })
            .collect::<Vec<_>>();
        let doc = json!({"schema": "haw.plugins/1", "plugins": plugins});
        assert_eq!(doc["schema"], "haw.plugins/1");
        let names: Vec<&str> = doc["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p["name"].as_str())
            .collect();
        assert!(names.contains(&"compliance"));
    }

    #[test]
    fn parse_index_reads_a_canned_doc() {
        let json = r#"{
            "schema": "haw.plugins.index/1",
            "plugins": [
                {"name":"foo","crate":"haw-foo","git":"https://example.com/x","description":"the foo plugin"},
                {"name":"bar","crate":"haw-bar","git":"https://example.com/x","description":"the bar plugin"}
            ]
        }"#;
        let entries = parse_index(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "foo");
        assert_eq!(entries[0].krate.as_deref(), Some("haw-foo"));
        assert_eq!(entries[0].description, "the foo plugin");
        assert_eq!(entries[1].name, "bar");
    }

    #[test]
    fn parse_index_rejects_wrong_schema() {
        let json = r#"{"schema":"something.else/1","plugins":[]}"#;
        assert!(parse_index(json).is_err());
    }

    #[test]
    fn merge_remote_adds_remote_only_and_marks_source() {
        // A local set with just the catalog `aspice`.
        let installed: Vec<String> = Vec::new();
        let no_subs: Vec<(String, Vec<String>)> = Vec::new();
        let rows = plugin_rows(&installed, no_subs.iter().map(|(k, v)| (k, v)));
        let remote = vec![
            // Already-known catalog plugin: must NOT flip to source `remote`.
            RemoteEntry {
                name: "aspice".to_string(),
                krate: Some("haw-aspice".to_string()),
                git: Some("https://example.com/x".to_string()),
                description: "".to_string(),
            },
            // A remote-only plugin: appears as source `remote`, status available.
            RemoteEntry {
                name: "zeta".to_string(),
                krate: Some("haw-zeta".to_string()),
                git: Some("https://example.com/x".to_string()),
                description: "a community plugin".to_string(),
            },
        ];
        let merged = merge_remote(rows, &remote);

        let aspice = merged.iter().find(|r| r.name == "aspice").unwrap();
        assert_eq!(
            aspice.source, "catalog",
            "known plugin keeps catalog source"
        );

        let zeta = merged.iter().find(|r| r.name == "zeta").unwrap();
        assert_eq!(zeta.source, "remote");
        assert!(!zeta.installed);
        assert_eq!(zeta.description, "a community plugin");
    }

    #[test]
    fn seed_index_parses_as_the_first_party_and_starter_plugins() {
        let json = include_str!("../../../plugins-index.json");
        let entries = parse_index(json).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "aspice",
                "jira",
                "misra",
                "compliance",
                "artifact",
                "git-gate",
                "fleet-status",
                "docker",
                "web",
                "k8s",
            ]
        );
        // Every entry carries the first-party git source.
        assert!(
            entries
                .iter()
                .all(|e| e.git.as_deref() == Some("https://github.com/Nastwinns/hawser"))
        );
    }

    #[test]
    fn scaffold_shell_writes_executable_entry_with_schema() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("haw-foo");
        plugins_new("foo", PluginLang::Shell, Some(&target)).unwrap();
        let entry = target.join("haw-foo");
        assert!(entry.is_file(), "shell entry file exists");
        let body = std::fs::read_to_string(&entry).unwrap();
        assert!(body.contains("haw.plugin.report/1"));
        assert!(body.contains("HAW_JSON"));
        assert!(target.join("README.md").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&entry).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "entry script is executable");
        }
    }

    #[test]
    fn scaffold_python_writes_entry_and_readme() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("haw-foo");
        plugins_new("foo", PluginLang::Python, Some(&target)).unwrap();
        let body = std::fs::read_to_string(target.join("haw-foo")).unwrap();
        assert!(body.contains("haw.plugin.report/1"));
        assert!(body.contains("python3"));
        assert!(target.join("README.md").is_file());
    }

    #[test]
    fn scaffold_go_writes_main_and_gomod() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("haw-foo");
        plugins_new("foo", PluginLang::Go, Some(&target)).unwrap();
        let main = std::fs::read_to_string(target.join("main.go")).unwrap();
        assert!(main.contains("haw.plugin.report/1"));
        let gomod = std::fs::read_to_string(target.join("go.mod")).unwrap();
        assert!(gomod.contains("module haw-foo"));
    }

    #[test]
    fn scaffold_rust_writes_cargo_toml_and_main() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("haw-foo");
        plugins_new("foo", PluginLang::Rust, Some(&target)).unwrap();
        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert!(cargo.contains(r#"name = "haw-foo""#));
        assert!(cargo.contains("[[bin]]"));
        let main = std::fs::read_to_string(target.join("src/main.rs")).unwrap();
        assert!(main.contains("haw.plugin.report/1"));
    }

    #[test]
    fn scaffold_refuses_a_non_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("haw-foo");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("existing.txt"), b"keep me").unwrap();
        let err = plugins_new("foo", PluginLang::Shell, Some(&target)).unwrap_err();
        assert!(format!("{err:#}").contains("not empty"));
    }

    #[test]
    fn plugin_view_schema_renders_title_and_lines() {
        let stdout = json!({
            "schema": "haw.plugin.view/1",
            "title": "SBOM status",
            "lines": ["kernel ✓", "hal ✓"],
        })
        .to_string();
        let text = plugin_view_text("sbom", &stdout);
        assert_eq!(text, "SBOM status\nkernel ✓\nhal ✓\n");
    }

    #[test]
    fn non_view_stdout_falls_back_to_raw() {
        let text = plugin_view_text("sbom", "plain text panel\nsecond line\n");
        assert_eq!(text, "plain text panel\nsecond line\n");
    }

    #[test]
    fn empty_stdout_yields_a_note() {
        let text = plugin_view_text("ghost", "   ");
        assert!(text.contains("produced no output"));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tuning_tests {
    use super::*;

    fn ws_with(manifest: &str) -> Workspace {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MANIFEST_FILE);
        std::fs::write(&path, manifest).unwrap();
        let ws = Workspace::open_manifest(&path).unwrap();
        // Keep the tempdir alive for the workspace's lifetime by leaking it;
        // fine for a unit test.
        std::mem::forget(dir);
        ws
    }

    const WITH_DEFAULTS: &str = r#"
[defaults]
filter = "blob:none"
depth = 3

[repo.a]
url = "https://example.com/a.git"
rev = "main"
"#;

    #[test]
    fn manifest_defaults_apply_when_no_flag() {
        let ws = ws_with(WITH_DEFAULTS);
        let t = resolve_tuning(&ws, None, None, false);
        assert_eq!(t.filter.as_deref(), Some("blob:none"));
        assert_eq!(t.depth, Some(3));
    }

    #[test]
    fn cli_flags_override_manifest_defaults() {
        let ws = ws_with(WITH_DEFAULTS);
        let t = resolve_tuning(&ws, Some("tree:0".to_string()), Some(1), false);
        assert_eq!(t.filter.as_deref(), Some("tree:0"));
        assert_eq!(t.depth, Some(1));
    }

    #[test]
    fn no_manifest_defaults_no_flags_is_empty() {
        let ws = ws_with(
            r#"
[repo.a]
url = "https://example.com/a.git"
rev = "main"
"#,
        );
        let t = resolve_tuning(&ws, None, None, false);
        assert!(t.filter.is_none());
        assert!(t.depth.is_none());
        assert_eq!(t.submodules, None);
    }

    #[test]
    fn recurse_submodules_flag_overrides_to_true() {
        let ws = ws_with(WITH_DEFAULTS);
        let t = resolve_tuning(&ws, None, None, true);
        assert_eq!(t.submodules, Some(true));
    }

    #[test]
    fn no_flag_leaves_submodules_to_manifest() {
        // Tuning stays None so plan_sync falls back to the per-repo / defaults
        // value resolved in the resolver.
        let ws = ws_with(WITH_DEFAULTS);
        let t = resolve_tuning(&ws, None, None, false);
        assert_eq!(t.submodules, None);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod completions_tests {
    use super::*;
    use clap_complete::Shell;

    fn script_for(shell: Shell) -> String {
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        let mut buf: Vec<u8> = Vec::new();
        clap_complete::generate(shell, &mut cmd, name, &mut buf);
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn bash_completions_mention_haw() {
        let script = script_for(Shell::Bash);
        assert!(!script.is_empty());
        assert!(script.contains("haw"), "bash script: {script}");
    }

    #[test]
    fn zsh_completions_mention_haw() {
        let script = script_for(Shell::Zsh);
        assert!(!script.is_empty());
        assert!(script.contains("haw"), "zsh script: {script}");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod json_output_tests {
    use super::*;
    use haw_core::lock::{LOCK_VERSION, LockedRepo, Lockfile};

    #[test]
    fn lock_json_has_schema_and_revs() {
        let lockfile = Lockfile {
            version: LOCK_VERSION,
            repos: vec![LockedRepo {
                name: "kernel".to_string(),
                url: "https://example.com/kernel.git".to_string(),
                path: "kernel".into(),
                rev: "0123456789abcdef0123456789abcdef01234567".to_string(),
                source_rev: "main".to_string(),
                branch: "main".to_string(),
                groups: vec!["firmware".to_string()],
            }],
        };
        let value = lock_json(&lockfile);
        assert_eq!(value["schema"], "haw.lock/1");
        assert_eq!(
            value["repos"][0]["rev"],
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(value["repos"][0]["source_rev"], "main");
    }

    #[test]
    fn change_status_json_has_schema_and_repo() {
        let statuses = vec![change::ChangeRepoStatus {
            name: "kernel".to_string(),
            branch: "change/FEAT-42".to_string(),
            missing: false,
            on_branch: true,
            dirty: false,
            head: Some("deadbeef".to_string()),
        }];
        let prs = std::collections::HashMap::new();
        let value = change_status_value("FEAT-42", &statuses, &prs);
        assert_eq!(value["schema"], "haw.change-status/1");
        assert_eq!(value["id"], "FEAT-42");
        assert_eq!(value["repos"][0]["name"], "kernel");
        assert_eq!(value["repos"][0]["on_branch"], true);
    }

    #[test]
    fn build_json_has_schema_and_exit_code() {
        let rows = vec![
            ("kernel".to_string(), Some(0), true),
            ("hal".to_string(), Some(2), false),
        ];
        let value = build_test_value(true, &rows);
        assert_eq!(value["schema"], "haw.build/1");
        assert_eq!(value["repos"][0]["name"], "kernel");
        assert_eq!(value["repos"][0]["exit_code"], 0);
        assert_eq!(value["repos"][0]["ok"], true);
        assert_eq!(value["repos"][1]["exit_code"], 2);
        assert_eq!(value["repos"][1]["ok"], false);
    }

    #[test]
    fn test_json_has_test_schema() {
        let value = build_test_value(false, &[("kernel".to_string(), Some(0), true)]);
        assert_eq!(value["schema"], "haw.test/1");
        assert_eq!(value["repos"][0]["ok"], true);
    }

    #[test]
    fn change_start_json_has_schema_and_branch() {
        let repos = vec![
            change::ChangeRepo {
                name: "kernel".to_string(),
                branch: "change/FEAT-42".to_string(),
                pr_url: None,
                pr_number: None,
            },
            change::ChangeRepo {
                name: "hal".to_string(),
                branch: "change/FEAT-42".to_string(),
                pr_url: None,
                pr_number: None,
            },
        ];
        let value = change_start_value("FEAT-42", &repos);
        assert_eq!(value["schema"], "haw.change-start/1");
        assert_eq!(value["id"], "FEAT-42");
        assert_eq!(value["repos"][0]["name"], "kernel");
        assert_eq!(value["repos"][0]["branch"], "change/FEAT-42");
    }

    #[test]
    fn change_request_json_has_schema_and_url() {
        let outcomes = vec![
            orchestrate::RepoOutcome {
                name: "kernel".to_string(),
                url: Some("https://example.com/kernel/pull/1".to_string()),
                result: Ok("https://example.com/kernel/pull/1".to_string()),
            },
            orchestrate::RepoOutcome {
                name: "hal".to_string(),
                url: None,
                result: Err(orchestrate::RepoFailure::NoPr),
            },
        ];
        let value = change_request_value("FEAT-42", &outcomes);
        assert_eq!(value["schema"], "haw.change-request/1");
        assert_eq!(value["id"], "FEAT-42");
        assert_eq!(
            value["repos"][0]["url"],
            "https://example.com/kernel/pull/1"
        );
        assert_eq!(value["repos"][0]["ok"], true);
        assert_eq!(value["repos"][1]["ok"], false);
    }

    #[test]
    fn change_land_json_has_schema_and_merged() {
        let outcomes = vec![orchestrate::RepoOutcome {
            name: "kernel".to_string(),
            url: None,
            result: Ok("merged #1".to_string()),
        }];
        let value = change_land_value("FEAT-42", &outcomes);
        assert_eq!(value["schema"], "haw.change-land/1");
        assert_eq!(value["id"], "FEAT-42");
        assert_eq!(value["repos"][0]["merged"], "merged #1");
        assert_eq!(value["repos"][0]["ok"], true);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod demo_controller_tests {
    use super::*;
    use haw_tui::Controller;

    #[test]
    fn snapshot_has_expected_shape() {
        let mut controller = DemoController;
        let snapshot = controller.snapshot().expect("demo snapshot");
        assert_eq!(snapshot.stacks.len(), 2);
        assert_eq!(snapshot.fleet.len(), 2);
        let gateway = &snapshot
            .fleet
            .iter()
            .find(|(name, _)| name == "gateway")
            .expect("gateway stack")
            .1;
        assert_eq!(gateway.len(), 4);
        assert!(gateway.iter().any(|r| r.missing));
        assert!(gateway.iter().any(|r| r.drift));
        assert!(gateway.iter().any(|r| r.dirty));
        assert_eq!(snapshot.changesets.len(), 2);
        assert_eq!(snapshot.merges.len(), 1);
        assert!(snapshot.lock_present);
    }

    #[test]
    fn fleet_views_are_populated() {
        let mut controller = DemoController;
        assert!(!controller.fleet_prs().expect("prs").is_empty());
        assert!(!controller.fleet_ci().expect("ci").is_empty());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod files_tests {
    use super::{FILE_SIZE_CAP, render_file_bytes, safe_join};
    use std::path::PathBuf;

    /// A unique scratch directory under the OS temp dir, created for one test.
    fn scratch(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "haw-files-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn safe_join_walks_within_the_root() {
        let root = scratch("walk");
        std::fs::create_dir_all(root.join("src/net")).unwrap();
        std::fs::write(root.join("src/net/tcp.rs"), b"fn main() {}").unwrap();

        let joined = safe_join(&root, "src/net").unwrap();
        let real_root = root.canonicalize().unwrap();
        assert!(joined.starts_with(&real_root));
        assert!(joined.ends_with("src/net"));
        // "" resolves to the root itself.
        assert_eq!(safe_join(&root, "").unwrap(), real_root);

        let entries: Vec<String> = std::fs::read_dir(&joined)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["tcp.rs".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn safe_join_rejects_path_traversal() {
        let root = scratch("traverse");
        std::fs::create_dir_all(root.join("inside")).unwrap();
        // A sibling file outside the root, reachable only via `..`.
        std::fs::write(root.join("secret.txt"), b"top secret").unwrap();
        let escape = safe_join(&root.join("inside"), "../secret.txt");
        assert!(escape.is_err(), "traversal must be refused");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn render_file_bytes_sniffs_binary() {
        let out = render_file_bytes(b"ELF\0\x01\x02binary");
        assert!(out.starts_with("<binary file, "));
        assert!(out.contains("bytes>"));
    }

    #[test]
    fn render_file_bytes_returns_text_and_caps_lines() {
        assert_eq!(render_file_bytes(b"hello\nworld"), "hello\nworld");
        let many = (0..800)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let capped = render_file_bytes(many.as_bytes());
        assert!(capped.contains("truncated"));
        assert!(!capped.contains("line 799"));
    }

    #[test]
    fn file_size_cap_is_about_one_megabyte() {
        assert_eq!(FILE_SIZE_CAP, 1_048_576);
    }
}
