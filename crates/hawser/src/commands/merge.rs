//! `haw merge` command handlers: plan, resolve, status, cleanup, abort.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use haw_core::git::GitBackend;
use haw_core::workspace::Workspace;
use haw_git::ShellGit;

use crate::ui::palette::Palette;
use crate::{TakeSide, open_workspace, record};

/// Resolve which repo the merge acts on and its absolute checkout path.
/// Defaults to the sole repo when the manifest has exactly one.
pub(crate) fn merge_repo(ws: &Workspace, repo: Option<&str>) -> Result<(String, PathBuf)> {
    let name = match repo {
        Some(name) => name.to_string(),
        None => {
            let mut names = ws.manifest.repos.keys();
            match (names.next(), names.next()) {
                (Some(only), None) => only.clone(),
                _ => bail!(
                    "pass --repo (manifest has {} repos)",
                    ws.manifest.repos.len()
                ),
            }
        }
    };
    let spec = ws
        .manifest
        .repos
        .get(&name)
        .with_context(|| format!("repo `{name}` is not in the manifest"))?;
    let path = ws.root.join(spec.checkout_path(&name));
    if !ShellGit.is_repo(&path) {
        bail!(
            "repo `{name}` is not cloned at {}; run `haw sync`",
            path.display()
        );
    }
    Ok((name, path))
}

pub(crate) fn merge_plan(source: &str, repo: Option<&str>, into: Option<&str>) -> Result<()> {
    let ws = open_workspace()?;
    let (name, path) = merge_repo(&ws, repo)?;
    let plan = haw_merge::plan(
        &haw_merge::git::GitMerge,
        &path,
        &ws.state_dir(),
        &name,
        source,
        into,
    )?;
    record(&ws, "merge.plan", Some(&name), None, Some(source));
    let c = Palette::new();
    println!(
        "{}",
        c.bold(&format!(
            "planned merge of `{}` into `{}` on `{}` ({} slice(s)):",
            plan.source,
            plan.target,
            plan.integration,
            plan.slices.len()
        ))
    );
    for slice in &plan.slices {
        println!(
            "  {} {}",
            c.name(&format!("{:<16}", slice.name)),
            c.dim(&format!("{} file(s)", slice.paths.len()))
        );
    }
    println!(
        "{}",
        c.dim("next: haw merge resolve <slice> [--take ours|theirs], then haw merge cleanup")
    );
    Ok(())
}

pub(crate) fn merge_resolve(slice: &str, repo: Option<&str>, take: Option<TakeSide>) -> Result<()> {
    let ws = open_workspace()?;
    let (name, path) = merge_repo(&ws, repo)?;
    let side = take.map(|t| match t {
        TakeSide::Ours => haw_merge::Side::Ours,
        TakeSide::Theirs => haw_merge::Side::Theirs,
    });
    let plan = haw_merge::resolve(
        &haw_merge::git::GitMerge,
        &path,
        &ws.state_dir(),
        &name,
        slice,
        side,
    )?;
    record(&ws, "merge.resolve", Some(&name), None, Some(slice));
    let c = Palette::new();
    let remaining = plan.unresolved();
    println!("{} resolved slice `{}`", c.ok("✓"), c.name(slice));
    if remaining.is_empty() {
        println!("{}", c.ok("all slices resolved — run `haw merge cleanup`"));
    } else {
        println!("remaining: {}", c.warn(&remaining.join(", ")));
    }
    Ok(())
}

pub(crate) fn merge_status(repo: Option<&str>) -> Result<()> {
    let ws = open_workspace()?;
    let (name, _) = merge_repo(&ws, repo)?;
    let Some(plan) = haw_merge::load_plan(&ws.state_dir(), &name)? else {
        println!("no merge planned for `{name}` — start one with `haw merge plan <source>`");
        return Ok(());
    };
    let c = Palette::new();
    println!(
        "{}",
        c.bold(&format!(
            "merge `{}` -> `{}` on `{}`",
            plan.source, plan.target, plan.integration
        ))
    );
    for slice in &plan.slices {
        let mark = if slice.resolved {
            c.ok("✓")
        } else {
            c.dim("·")
        };
        println!(
            "  {mark} {} {}",
            c.name(&format!("{:<16}", slice.name)),
            c.dim(&format!("{} file(s)", slice.paths.len()))
        );
    }
    Ok(())
}

pub(crate) fn merge_cleanup(repo: Option<&str>, message: Option<&str>) -> Result<()> {
    let ws = open_workspace()?;
    let (name, path) = merge_repo(&ws, repo)?;
    let report = haw_merge::cleanup(
        &haw_merge::git::GitMerge,
        &path,
        &ws.state_dir(),
        &name,
        message,
    )?;
    record(
        &ws,
        "merge.cleanup",
        Some(&name),
        None,
        Some(&report.merge_sha),
    );
    let c = Palette::new();
    println!(
        "{} {}",
        c.ok("✓"),
        c.bold(&format!(
            "merged {} slice(s) into `{}` ({}); dropped `{}`",
            report.slices,
            report.target,
            &report.merge_sha[..8.min(report.merge_sha.len())],
            report.integration
        ))
    );
    Ok(())
}

pub(crate) fn merge_abort(repo: Option<&str>) -> Result<()> {
    let ws = open_workspace()?;
    let (name, path) = merge_repo(&ws, repo)?;
    let plan = haw_merge::abort(&haw_merge::git::GitMerge, &path, &ws.state_dir(), &name)?;
    record(&ws, "merge.abort", Some(&name), None, Some(&plan.source));
    println!(
        "aborted merge of `{}`; back on `{}`",
        plan.source, plan.target
    );
    Ok(())
}
