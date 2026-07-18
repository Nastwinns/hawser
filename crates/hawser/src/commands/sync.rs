//! `haw sync` command handler: clone/update every repo and write the lockfile.

use anyhow::{Context, Result, bail};
use haw_core::hooks;
use haw_core::workspace::{SyncOutcome, sync_repo};
use haw_git::ShellGit;
use haw_git::parallel::fan_out;
use serde_json::json;

use crate::ui::palette::Palette;
use crate::{default_jobs, open_workspace, record, resolve_tuning};

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync(
    stack: Option<&str>,
    overlays: &[String],
    groups: &[String],
    shared: bool,
    locked: bool,
    filter: Option<String>,
    depth: Option<u32>,
    recurse_submodules: bool,
    jobs: Option<usize>,
) -> Result<()> {
    let ws = open_workspace()?;
    let stack = ws.pick_stack(stack)?;
    if locked && !ws.lock_path().exists() {
        bail!("--locked: no haw.lock — commit one (haw lock) before running CI syncs");
    }
    hooks::fire(&ws, hooks::Hook::PreSync, &json!({"stack": stack}))?;
    let backend = ShellGit;
    let cache_root = if shared {
        let root = haw_git::default_cache_root().context("no cache directory on this platform")?;
        println!("sharing objects via {}", root.display());
        Some(root)
    } else {
        None
    };
    // CLI flag overrides the manifest `[defaults]`; fall back to the manifest.
    let tuning = resolve_tuning(&ws, filter, depth, recurse_submodules);
    let plan = ws.plan_sync(
        &stack,
        overlays,
        groups,
        cache_root.as_deref(),
        &tuning,
        &backend,
    )?;
    if plan.wrote_lock {
        println!("wrote haw.lock ({} repos pinned)", plan.tasks.len());
        record(&ws, "lock.write", None, None, None);
    } else if !overlays.is_empty() {
        println!("note: haw.lock exists — overlays ignored (run `haw lock` to re-resolve)");
    }

    let results = fan_out(&plan.tasks, default_jobs(jobs), |task| {
        sync_repo(task, &backend)
    });

    let c = Palette::new();
    let width = plan.tasks.iter().map(|t| t.name.len()).max().unwrap_or(4);
    let mut failures = 0usize;
    for (task, result) in plan.tasks.iter().zip(&results) {
        match result {
            Ok(outcome) => {
                let verb = match outcome {
                    SyncOutcome::Cloned => "cloned",
                    SyncOutcome::Updated => "updated",
                    SyncOutcome::AlreadySynced => "up to date",
                };
                println!(
                    "  {} {}  {}",
                    c.ok("✓"),
                    c.name(&format!("{:<width$}", task.name)),
                    c.dim(verb)
                );
                if *outcome != SyncOutcome::AlreadySynced {
                    record(&ws, "sync", Some(&task.name), None, Some(&task.target));
                }
            }
            Err(err) => {
                failures += 1;
                eprintln!("  {} {}  {err}", c.err("✗"), task.name);
            }
        }
    }
    println!(
        "{}",
        c.bold(&format!(
            "synced stack `{}` ({}/{} repos)",
            plan.stack,
            results.len() - failures,
            results.len()
        ))
    );
    if failures > 0 {
        bail!("{failures} repo(s) failed to sync");
    }
    hooks::fire(&ws, hooks::Hook::PostSync, &json!({"stack": plan.stack}))?;
    Ok(())
}
