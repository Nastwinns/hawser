//! `haw change` command handlers (start/status/request/land/goto/list),
//! snapshot save/restore/list, and the JSON document builders they emit.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use haw_core::workspace::Workspace;
use haw_core::{change, hooks, snapshot};
use haw_forge::{PrState, Tokens, orchestrate};
use haw_git::ShellGit;
use serde_json::json;

use crate::ui::palette::Palette;
use crate::{fail_exit, fire_phase, open_workspace, record};

/// The `haw.change-start/1` document: the changeset id and its per-repo
/// branches. Pure, so it is unit-testable without a workspace.
pub(crate) fn change_start_value(id: &str, repos: &[change::ChangeRepo]) -> serde_json::Value {
    let repos = repos
        .iter()
        .map(|r| json!({"name": r.name, "branch": r.branch}))
        .collect::<Vec<_>>();
    json!({
        "schema": "haw.change-start/1",
        "id": id,
        "repos": repos,
    })
}

pub(crate) fn change_start(
    id: &str,
    repos: Option<&[String]>,
    branch: Option<&str>,
    skip_branch: bool,
    labels: &[String],
    format: &str,
) -> Result<()> {
    if format != "text" && format != "json" {
        bail!("unknown format `{format}` (use text or json)");
    }
    let ws = open_workspace()?;
    let changeset = change::start(&ws, &ShellGit, id, repos, branch, skip_branch, labels)?;
    record(&ws, "change.start", None, None, Some(id));
    hooks::fire(&ws, hooks::Hook::PostChangeStart, &json!({"id": id}))?;
    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&change_start_value(&changeset.id, &changeset.repos))?
        );
        return Ok(());
    }
    let c = Palette::new();
    println!(
        "{}",
        c.bold(&format!(
            "changeset `{}` started across {} repo(s):",
            changeset.id,
            changeset.repos.len()
        ))
    );
    let width = changeset
        .repos
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(4);
    for repo in &changeset.repos {
        println!(
            "  {}  {} {}",
            c.name(&format!("{:<width$}", repo.name)),
            c.dim("->"),
            c.rev(&repo.branch)
        );
    }
    Ok(())
}

pub(crate) fn render_pr_state(state: PrState) -> &'static str {
    match state {
        PrState::Open => "open",
        PrState::Draft => "draft",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
    }
}

pub(crate) fn render_ci_status(status: haw_forge::CiStatus) -> &'static str {
    match status {
        haw_forge::CiStatus::Passed => "passed",
        haw_forge::CiStatus::Failed => "failed",
        haw_forge::CiStatus::Running => "running",
        haw_forge::CiStatus::Queued => "queued",
        haw_forge::CiStatus::Cancelled => "cancelled",
    }
}

/// `github`/`gitlab`/`—` for a manifest repo, from its remote URL.
pub(crate) fn forge_label(ws: &Workspace, name: &str) -> String {
    ws.manifest
        .repos
        .get(name)
        .and_then(|repo| repo.clone_url(&ws.manifest.remotes))
        .map(|url| match haw_forge::detect(&url) {
            haw_forge::ForgeKind::GitHub => "github".to_string(),
            haw_forge::ForgeKind::GitLab => "gitlab".to_string(),
            haw_forge::ForgeKind::Bitbucket => "bitbucket".to_string(),
            haw_forge::ForgeKind::Unknown => "—".to_string(),
        })
        .unwrap_or_else(|| "—".to_string())
}

/// Machine-readable `haw change status` (schema `haw.change-status/1`):
/// per-repo branch/dirty/head plus PR/MR + CI status when PRs exist.
pub(crate) fn change_status_json(
    ws: &Workspace,
    id: &str,
    statuses: &[change::ChangeRepoStatus],
) -> Result<()> {
    let changeset = change::Changeset::load(ws, id)?;
    let prs: std::collections::HashMap<String, serde_json::Value> =
        if changeset.repos.iter().any(|r| r.pr_number.is_some()) {
            let tokens = Tokens::from_env();
            orchestrate::statuses(ws, &tokens, id)?
                .into_iter()
                .map(|(name, status)| {
                    let value = match status {
                        None => serde_json::Value::Null,
                        Some(Ok(s)) => json!({
                            "state": render_pr_state(s.state),
                            "approved": s.approved,
                            "ci": match s.ci_passing {
                                Some(true) => "passing",
                                Some(false) => "failing",
                                None => "pending",
                            },
                            "url": s.url,
                        }),
                        Some(Err(err)) => json!({"error": err.to_string()}),
                    };
                    (name, value)
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        };

    let value = change_status_value(id, statuses, &prs);
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Build the `haw.change-status/1` document from per-repo statuses and an
/// (optional) map of per-repo PR/CI info. Pure, so it is unit-testable
/// without a workspace or network.
pub(crate) fn change_status_value(
    id: &str,
    statuses: &[change::ChangeRepoStatus],
    prs: &std::collections::HashMap<String, serde_json::Value>,
) -> serde_json::Value {
    let repos = statuses
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "branch": s.branch,
                "missing": s.missing,
                "on_branch": s.on_branch,
                "dirty": s.dirty,
                "head": s.head,
                "pr": prs.get(&s.name).cloned().unwrap_or(serde_json::Value::Null),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": "haw.change-status/1",
        "id": id,
        "repos": repos,
    })
}

pub(crate) fn change_status(id: &str, format: &str) -> Result<()> {
    let ws = open_workspace()?;
    let statuses = change::status(&ws, &ShellGit, id)?;

    if format == "json" {
        return change_status_json(&ws, id, &statuses);
    }
    if format != "text" {
        bail!("unknown format `{format}` (use text or json)");
    }

    let c = Palette::new();
    let width = statuses.iter().map(|s| s.name.len()).max().unwrap_or(4);
    println!("{}", c.bold(&format!("changeset `{id}`")));
    println!(
        "{}",
        c.header(&format!(
            "{:<width$}  {:<24} {:<9} {:<6} {:<10} PR",
            "REPO", "BRANCH", "ON IT", "DIRTY", "HEAD"
        ))
    );
    for s in &statuses {
        if s.missing {
            println!(
                "{}  {}",
                c.name(&format!("{:<width$}", s.name)),
                c.dim("(repo missing — run `haw sync`)")
            );
            continue;
        }
        println!(
            "{}  {}  {} {} {} —",
            c.name(&format!("{:<width$}", s.name)),
            c.rev(&format!("{:<24}", s.branch)),
            if s.on_branch {
                c.ok(&format!("{:<9}", "yes"))
            } else {
                c.err(&format!("{:<9}", "NO"))
            },
            if s.dirty {
                c.warn(&format!("{:<6}", "yes"))
            } else {
                c.ok(&format!("{:<6}", "-"))
            },
            c.dim(&format!(
                "{:<10}",
                s.head
                    .as_deref()
                    .map(|h| &h[..8.min(h.len())])
                    .unwrap_or("—")
            )),
        );
    }

    let changeset = change::Changeset::load(&ws, id)?;
    if changeset.repos.iter().any(|r| r.pr_number.is_some()) {
        println!();
        println!("PR/MRs:");
        let tokens = Tokens::from_env();
        for (name, status) in orchestrate::statuses(&ws, &tokens, id)? {
            match status {
                None => println!("  {name}  (no PR — run `haw change request`)"),
                Some(Ok(s)) => println!(
                    "  {name}  {}  approved: {}  ci: {}  {}",
                    render_pr_state(s.state),
                    if s.approved { "yes" } else { "no" },
                    match s.ci_passing {
                        Some(true) => "passing",
                        Some(false) => "FAILING",
                        None => "pending",
                    },
                    s.url
                ),
                Some(Err(err)) => println!("  {name}  (status unavailable: {err})"),
            }
        }
    } else {
        println!("(no PR/MRs yet — open them with `haw change request {id}`)");
    }
    Ok(())
}

/// The `haw.change-request/1` document: per-repo PR/MR url (or error) + ok.
/// Pure, so it is unit-testable without a workspace or network.
pub(crate) fn change_request_value(
    id: &str,
    outcomes: &[orchestrate::RepoOutcome],
) -> serde_json::Value {
    let repos = outcomes
        .iter()
        .map(|o| match &o.result {
            Ok(url) => json!({"name": o.name, "url": url, "ok": true}),
            Err(err) => json!({"name": o.name, "error": err.to_string(), "ok": false}),
        })
        .collect::<Vec<_>>();
    json!({
        "schema": "haw.change-request/1",
        "id": id,
        "repos": repos,
    })
}

pub(crate) fn change_request(id: &str, base: Option<&str>, format: &str) -> Result<ExitCode> {
    if format != "text" && format != "json" {
        bail!("unknown format `{format}` (use text or json)");
    }
    let ws = open_workspace()?;
    fire_phase(
        &ws,
        hooks::Hook::PreRequest,
        json!({"id": id, "base": base}),
    )?;
    let tokens = Tokens::from_env();
    let outcomes = orchestrate::request(&ws, &ShellGit, &tokens, id, base, None)?;
    let mut failures = 0usize;
    for outcome in &outcomes {
        match &outcome.result {
            Ok(url) => record(&ws, "change.request", Some(&outcome.name), None, Some(url)),
            Err(_) => failures += 1,
        }
    }
    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&change_request_value(id, &outcomes))?
        );
        return Ok(fail_exit(failures));
    }
    let c = Palette::new();
    for outcome in &outcomes {
        match &outcome.result {
            Ok(url) => println!("  {} {}  {}", c.ok("✓"), c.name(&outcome.name), c.dim(url)),
            Err(err) => eprintln!("  {} {}  {err}", c.err("✗"), outcome.name),
        }
    }
    if failures > 0 {
        bail!("{failures} repo(s) failed; fix and re-run `haw change request {id}`");
    }
    println!(
        "requested changeset `{id}` ({} PR/MRs, cross-linked)",
        outcomes.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// The `haw.change-land/1` document: per-repo merge result + ok.
/// Pure, so it is unit-testable without a workspace or network.
pub(crate) fn change_land_value(
    id: &str,
    outcomes: &[orchestrate::RepoOutcome],
) -> serde_json::Value {
    let repos = outcomes
        .iter()
        .map(|o| match &o.result {
            Ok(msg) => json!({"name": o.name, "merged": msg, "ok": true}),
            Err(err) => json!({"name": o.name, "error": err.to_string(), "ok": false}),
        })
        .collect::<Vec<_>>();
    json!({
        "schema": "haw.change-land/1",
        "id": id,
        "repos": repos,
    })
}

pub(crate) fn change_land(id: &str, format: &str) -> Result<ExitCode> {
    if format != "text" && format != "json" {
        bail!("unknown format `{format}` (use text or json)");
    }
    let ws = open_workspace()?;
    let tokens = Tokens::from_env();
    let outcomes = orchestrate::land(&ws, &tokens, id)?;
    let mut failed = false;
    for outcome in &outcomes {
        match &outcome.result {
            Ok(_) => record(&ws, "change.land", Some(&outcome.name), None, Some(id)),
            Err(_) => failed = true,
        }
    }
    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&change_land_value(id, &outcomes))?
        );
        if !failed {
            fire_phase(
                &ws,
                hooks::Hook::PostLand,
                json!({"id": id, "repos": outcomes.len()}),
            )?;
        }
        return Ok(fail_exit(usize::from(failed)));
    }
    let c = Palette::new();
    for outcome in &outcomes {
        match &outcome.result {
            Ok(msg) => println!("  {} {}  {}", c.ok("✓"), c.name(&outcome.name), c.dim(msg)),
            Err(err) => eprintln!("  {} {}  {err}", c.err("✗"), outcome.name),
        }
    }
    if failed {
        bail!("landing stopped at the first failure; later repos stay unmerged");
    }
    fire_phase(
        &ws,
        hooks::Hook::PostLand,
        json!({"id": id, "repos": outcomes.len()}),
    )?;
    println!("changeset `{id}` landed ({} repos)", outcomes.len());
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn change_goto(id: &str, repo: Option<&str>) -> Result<()> {
    let ws = open_workspace()?;
    let changeset = change::Changeset::load(&ws, id)?;
    let path_of = |name: &str| -> Result<PathBuf> {
        let spec = ws
            .manifest
            .repos
            .get(name)
            .with_context(|| format!("repo `{name}` is not in the manifest"))?;
        Ok(ws.root.join(spec.checkout_path(name)))
    };

    let name = match repo {
        Some(name) => {
            if !changeset.repos.iter().any(|r| r.name == name) {
                bail!("repo `{name}` is not part of changeset `{id}`");
            }
            name.to_string()
        }
        None if std::io::stdin().is_terminal() => {
            for (index, entry) in changeset.repos.iter().enumerate() {
                eprintln!("  {}. {}  ({})", index + 1, entry.name, entry.branch);
            }
            eprint!("repo number: ");
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            let choice: usize = line.trim().parse().context("not a number")?;
            changeset
                .repos
                .get(choice.saturating_sub(1))
                .map(|entry| entry.name.clone())
                .context("choice out of range")?
        }
        None => {
            let names: Vec<&str> = changeset.repos.iter().map(|r| r.name.as_str()).collect();
            bail!(
                "pass a repo name (one of: {}) — interactive picker needs a terminal",
                names.join(", ")
            );
        }
    };
    println!("{}", path_of(&name)?.display());
    Ok(())
}

pub(crate) fn snapshot_save(name: &str) -> Result<()> {
    let ws = open_workspace()?;
    let snap = snapshot::save(&ws, &ShellGit, name)?;
    record(&ws, "snapshot.save", None, None, Some(name));
    println!("saved snapshot `{name}` ({} repos)", snap.repos.len());
    for repo in &snap.repos {
        println!(
            "  {}  {}  ({})",
            repo.name,
            &repo.sha[..8.min(repo.sha.len())],
            repo.branch.as_deref().unwrap_or("detached")
        );
    }
    Ok(())
}

pub(crate) fn snapshot_restore(name: &str) -> Result<()> {
    let ws = open_workspace()?;
    let snap = snapshot::restore(&ws, &ShellGit, name)?;
    record(&ws, "snapshot.restore", None, None, Some(name));
    println!("restored snapshot `{name}` ({} repos)", snap.repos.len());
    Ok(())
}

pub(crate) fn snapshot_list() -> Result<()> {
    let ws = open_workspace()?;
    let names = snapshot::Snapshot::list(&ws)?;
    if names.is_empty() {
        println!("no snapshots — save one with `haw change snapshot save <name>`");
        return Ok(());
    }
    for name in names {
        println!("{name}");
    }
    Ok(())
}

pub(crate) fn change_list() -> Result<()> {
    let ws = open_workspace()?;
    let ids = change::Changeset::list(&ws)?;
    if ids.is_empty() {
        println!("no changesets — start one with `haw change start <id>`");
        return Ok(());
    }
    for id in ids {
        println!("{id}");
    }
    Ok(())
}
