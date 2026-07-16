//! `haw-git-gate` — a pre-commit-style hygiene and secret gate.
//!
//! Reads a `haw.plugin/1` context (from `HAW_JSON`, falling back to stdin) and
//! orchestrates the secret/hygiene checks that are actually available on the
//! host: it prefers `gitleaks` when present and otherwise runs a small,
//! clearly-labelled heuristic scan. It is fail-open on a missing tool,
//! fail-closed on a real finding, and never reports false confidence.
//!
//! It prints a `haw.plugin.report/1` report to stdout and exits nonzero when the
//! gate does not pass.

mod cli;
mod context;
mod report;
mod scan;

use std::path::PathBuf;
use std::process::ExitCode;

use report::{Finding, Report};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match cli::parse(&args) {
        Ok(cli::ParseOutcome::Help(text)) => {
            print!("{text}");
            return ExitCode::SUCCESS;
        }
        Ok(cli::ParseOutcome::Run(opts)) => opts,
        Err(err) => {
            eprintln!("haw-git-gate: {err}");
            return ExitCode::FAILURE;
        }
    };

    let report = match run(opts) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("haw-git-gate: {err}");
            return ExitCode::FAILURE;
        }
    };

    match serde_json::to_string(&report) {
        Ok(json) => println!("{json}"),
        Err(err) => {
            eprintln!("haw-git-gate: failed to serialize report: {err}");
            return ExitCode::FAILURE;
        }
    }

    if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Run the gate end to end, returning the report to be printed.
fn run(opts: cli::Options) -> Result<Report, String> {
    let raw = context::read_raw_context()?;
    let ctx = context::Context::from_json(&raw)?;

    let targets = select_targets(&ctx, opts.repo.as_deref())?;
    let gitleaks = scan::gitleaks_available();

    let mut findings: Vec<Finding> = Vec::new();

    for target in &targets {
        findings.push(Finding::info(format!(
            "scanning repo '{}' at {}",
            target.name,
            target.path.display()
        )));
        if gitleaks {
            findings.extend(scan::run_gitleaks(&target.path));
        } else {
            findings.extend(scan::heuristic_secret_scan(&target.path));
        }
        findings.extend(scan::hygiene_scan(&target.path));
    }

    if !gitleaks {
        findings.push(Finding::info(
            "heuristic scan only — install gitleaks for real coverage",
        ));
    }

    let summary = build_summary(&targets, gitleaks, &findings);

    let mut report = Report {
        schema: "haw.plugin.report/1".to_string(),
        plugin: "haw-git-gate".to_string(),
        phase: opts.phase,
        ok: true,
        summary,
        artifacts: Vec::new(),
        findings,
    };
    report.recompute_ok();
    Ok(report)
}

/// A resolved scan target: a repo name paired with its path.
struct Target {
    name: String,
    path: PathBuf,
}

/// Resolve which repos to scan from the context and optional `--repo`.
fn select_targets(ctx: &context::Context, repo: Option<&str>) -> Result<Vec<Target>, String> {
    match repo {
        Some(explicit) => {
            let path = PathBuf::from(explicit);
            let name = ctx
                .repos
                .iter()
                .find(|r| r.path == path)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| explicit.to_string());
            Ok(vec![Target { name, path }])
        }
        None => {
            if ctx.repos.is_empty() {
                return Err("context contains no repos to scan".to_string());
            }
            Ok(ctx
                .repos
                .iter()
                .map(|r| Target {
                    name: r.name.clone(),
                    path: r.path.clone(),
                })
                .collect())
        }
    }
}

/// Build the one-line human summary.
fn build_summary(targets: &[Target], gitleaks: bool, findings: &[Finding]) -> String {
    let errors = findings.iter().filter(|f| f.level == "error").count();
    let warns = findings.iter().filter(|f| f.level == "warn").count();
    let engine = if gitleaks {
        "gitleaks"
    } else {
        "heuristic fallback"
    };
    format!(
        "scanned {} repo(s) with {engine}: {errors} error(s), {warns} warning(s)",
        targets.len()
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn init_repo(dir: &Path) {
        let ok = std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "git init failed");
    }

    fn scan_repo(dir: &Path) -> Report {
        let ctx = context::Context {
            repos: vec![context::Repo {
                name: "fixture".to_string(),
                path: dir.to_path_buf(),
            }],
        };
        let targets = select_targets(&ctx, None).unwrap();
        let gitleaks = scan::gitleaks_available();
        let mut findings: Vec<Finding> = Vec::new();
        for target in &targets {
            if gitleaks {
                findings.extend(scan::run_gitleaks(&target.path));
            } else {
                findings.extend(scan::heuristic_secret_scan(&target.path));
            }
            findings.extend(scan::hygiene_scan(&target.path));
        }
        if !gitleaks {
            findings.push(Finding::info(
                "heuristic scan only — install gitleaks for real coverage",
            ));
        }
        let mut report = Report {
            schema: "haw.plugin.report/1".to_string(),
            plugin: "haw-git-gate".to_string(),
            phase: None,
            ok: true,
            summary: build_summary(&targets, gitleaks, &findings),
            artifacts: Vec::new(),
            findings,
        };
        report.recompute_ok();
        report
    }

    #[test]
    fn planted_aws_key_triggers_a_finding() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        fs::write(
            tmp.path().join("config.txt"),
            "aws_access_key_id = AKIAIOSFODNN7EXAMPLE\n",
        )
        .unwrap();
        let report = scan_repo(tmp.path());
        let hit = report.findings.iter().any(|f| {
            f.message.to_lowercase().contains("aws") || f.message.to_lowercase().contains("secret")
        });
        assert!(
            hit,
            "expected an AWS key finding, got: {:?}",
            summary_of(&report)
        );
    }

    #[test]
    fn clean_repo_yields_ok_true() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        fs::write(tmp.path().join("README.md"), "# clean\n").unwrap();
        let report = scan_repo(tmp.path());
        assert!(
            report.ok,
            "clean repo should pass: {:?}",
            summary_of(&report)
        );
        assert!(
            !report.findings.iter().any(|f| f.level == "error"),
            "clean repo should have no errors"
        );
    }

    #[test]
    fn absence_of_gitleaks_adds_heuristic_notice() {
        if scan::gitleaks_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        fs::write(tmp.path().join("README.md"), "# clean\n").unwrap();
        let report = scan_repo(tmp.path());
        let notice = report
            .findings
            .iter()
            .any(|f| f.level == "info" && f.message.contains("heuristic scan only"));
        assert!(notice, "expected the heuristic-only info notice");
    }

    #[test]
    fn private_key_header_is_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        fs::write(
            tmp.path().join("id_rsa"),
            "-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----\n",
        )
        .unwrap();
        let report = scan_repo(tmp.path());
        let hit = report
            .findings
            .iter()
            .any(|f| f.message.to_lowercase().contains("private key"));
        assert!(hit, "expected a private key finding");
    }

    fn summary_of(report: &Report) -> Vec<String> {
        report
            .findings
            .iter()
            .map(|f| format!("{}: {}", f.level, f.message))
            .collect()
    }
}
