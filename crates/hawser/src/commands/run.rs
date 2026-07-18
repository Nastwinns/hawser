//! `haw build` / `haw test` command handlers, plus the live-streaming runner.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, bail};
use haw_core::git::GitBackend;
use haw_core::{hooks, resolver};
use haw_git::ShellGit;
use haw_git::parallel::fan_out;
use serde_json::json;

use crate::ui::palette::Palette;
use crate::{default_jobs, fail_exit, fire_phase, open_workspace, shell_command};

/// Run one repo's build/test command, streaming stdout+stderr LIVE with a
/// `<repo> │` prefix on every line. `lock` serializes whole lines so parallel
/// repos never interleave mid-line. Returns the process exit status.
pub(crate) fn stream_repo(
    name: &str,
    path: &Path,
    cmd: &str,
    lock: &std::sync::Mutex<()>,
    c: &Palette,
) -> std::io::Result<std::process::ExitStatus> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    let mut child = shell_command(cmd)
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Distinct, stable color per repo (docker-compose style) so parallel streams
    // are easy to tell apart. Deterministic from the name; plain under NO_COLOR.
    const REPO_COLORS: &[&str] = &["36", "33", "32", "35", "34", "96", "93", "95"];
    let color = REPO_COLORS[name.bytes().map(usize::from).sum::<usize>() % REPO_COLORS.len()];
    let prefix = format!("{} {}", c.paint(&format!("1;{color}"), name), c.dim("│"));
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    std::thread::scope(|scope| {
        if let Some(stderr) = stderr {
            scope.spawn(|| {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    eprintln!("{prefix} {line}");
                }
            });
        }
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                println!("{prefix} {line}");
            }
        }
    });
    child.wait()
}

/// The `haw.build/1` / `haw.test/1` document from per-repo (name, exit_code, ok)
/// rows. Pure, so it is unit-testable without a workspace or subprocesses.
pub(crate) fn build_test_value(
    build: bool,
    rows: &[(String, Option<i32>, bool)],
) -> serde_json::Value {
    let repos = rows
        .iter()
        .map(|(name, code, ok)| json!({"name": name, "exit_code": code, "ok": ok}))
        .collect::<Vec<_>>();
    json!({
        "schema": if build { "haw.build/1" } else { "haw.test/1" },
        "repos": repos,
    })
}

pub(crate) fn build_or_test(
    build: bool,
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
    let verb = if build { "build" } else { "test" };
    let (pre, post) = if build {
        (hooks::Hook::PreBuild, hooks::Hook::PostBuild)
    } else {
        (hooks::Hook::PreTest, hooks::Hook::PostTest)
    };
    fire_phase(&ws, pre, json!({"groups": groups}))?;
    let targets: Vec<(String, PathBuf, String)> = ws
        .manifest
        .repos
        .iter()
        .filter(|(_, repo)| resolver::group_match(&repo.groups, groups))
        .filter_map(|(name, repo)| {
            let cmd = if build { &repo.build } else { &repo.test };
            cmd.as_ref().map(|cmd| {
                (
                    name.clone(),
                    ws.root.join(repo.checkout_path(name)),
                    cmd.clone(),
                )
            })
        })
        .filter(|(_, path, _)| backend.is_repo(path))
        .collect();
    if targets.is_empty() {
        bail!("no cloned repo declares a `{verb}` command in the manifest");
    }

    // In JSON mode, capture each repo's output (one machine doc on stdout) rather
    // than streaming live text that would clobber the document.
    if as_json {
        let results = fan_out(&targets, default_jobs(jobs), |(name, path, cmd)| {
            let output = shell_command(cmd).current_dir(path).output();
            (name.clone(), output)
        });
        let mut failures = 0usize;
        let rows = results
            .iter()
            .map(|(name, output)| match output {
                Ok(out) => {
                    let ok = out.status.success();
                    if !ok {
                        failures += 1;
                    }
                    (name.clone(), out.status.code(), ok)
                }
                Err(_) => {
                    failures += 1;
                    (name.clone(), None, false)
                }
            })
            .collect::<Vec<_>>();
        let total = rows.len();
        println!(
            "{}",
            serde_json::to_string_pretty(&build_test_value(build, &rows))?
        );
        fire_phase(&ws, post, json!({"failures": failures, "total": total}))?;
        return Ok(fail_exit(failures));
    }

    let c = Palette::new();
    // Stream each repo's output LIVE, prefixed with the repo name (docker-compose
    // / k9s style), instead of buffering and printing after each finishes. A shared
    // lock keeps whole lines atomic so parallel repos never interleave mid-line.
    let print_lock = std::sync::Mutex::new(());
    let results = fan_out(&targets, default_jobs(jobs), |(name, path, cmd)| {
        let status = stream_repo(name, path, cmd, &print_lock, &c);
        (name.clone(), status)
    });
    let total = results.len();
    let mut failures = 0usize;
    for (name, status) in &results {
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                failures += 1;
                eprintln!(
                    "{} {} {}",
                    c.err("✗"),
                    c.name(name),
                    c.dim(&format!("({s})"))
                );
            }
            Err(err) => {
                failures += 1;
                eprintln!(
                    "{} {} {}",
                    c.err("✗"),
                    c.name(name),
                    c.dim(&format!("(failed to run: {err})"))
                );
            }
        }
    }
    println!("{verb} ran in {}/{} repos", total - failures, total);
    fire_phase(&ws, post, json!({"failures": failures, "total": total}))?;
    if failures > 0 {
        bail!("{verb} failed in {failures} repo(s)");
    }
    Ok(ExitCode::SUCCESS)
}
