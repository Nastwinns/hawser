//! `haw-misra` — a MISRA C static-analysis gate across the fleet.
//!
//! Reads a `haw.plugin/1` context (from `HAW_JSON`, falling back to stdin) and
//! runs a MISRA C pass over each repo's tracked C/C++ sources by shelling out
//! to `cppcheck --addon=misra`, the common open-source MISRA checker.
//!
//! It has two faces:
//!  - As the `haw misra` subcommand it prints a human summary (files scanned +
//!    violation count), or the raw report with `--format json`.
//!  - As a `pre-request` lifecycle hook it emits one `haw.plugin.report/1`
//!    document: `ok:false` with an `error`-level finding per violation (so it
//!    BLOCKS the PR) when violations exist, `ok:true` otherwise.
//!
//! It is **fail-open**: if `cppcheck` is not on PATH — or a repo has no C/C++
//! files — it reports `ok:true` with a `warn` and exits 0, so a missing tool
//! never blocks adoption.

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
            eprintln!("haw-misra: {err}");
            return ExitCode::FAILURE;
        }
    };

    // In hook mode (a phase was passed) always emit JSON so haw can parse it.
    let emit_json = opts.phase.is_some() || opts.format == cli::Format::Json;

    let outcome = match run(&opts) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("haw-misra: {err}");
            return ExitCode::FAILURE;
        }
    };

    if emit_json {
        match serde_json::to_string(&outcome.report) {
            Ok(json) => println!("{json}"),
            Err(err) => {
                eprintln!("haw-misra: failed to serialize report: {err}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        print!("{}", human_summary(&outcome));
    }

    if outcome.report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A resolved scan target: a repo name paired with its path.
struct Target {
    name: String,
    path: PathBuf,
}

/// The result of a run: the report plus the totals for the human summary.
struct Outcome {
    report: Report,
    files_scanned: usize,
    repos_scanned: usize,
    cppcheck: bool,
}

/// Run the gate end to end, returning the report and human-summary totals.
fn run(opts: &cli::Options) -> Result<Outcome, String> {
    let raw = context::read_raw_context()?;
    let ctx = context::Context::from_json(&raw)?;
    let targets = select_targets(&ctx, opts.repo.as_deref());

    let cppcheck = scan::cppcheck_available();
    let mut findings: Vec<Finding> = Vec::new();
    let mut files_scanned = 0usize;

    if let Some(root) = &ctx.root {
        findings.push(Finding::info(format!("fleet root: {}", root.display())));
    }

    if !cppcheck {
        // Fail-open: a missing checker never blocks. One warn, ok stays true.
        findings.push(Finding::warn("cppcheck not found; MISRA gate skipped"));
    } else if targets.is_empty() {
        findings.push(Finding::warn("no repos in context; MISRA gate skipped"));
    } else {
        for target in &targets {
            let scan = scan::run_misra(&target.name, &target.path);
            files_scanned += scan.files_scanned;
            findings.extend(scan.findings);
        }
    }

    let repos_scanned = targets.len();
    let summary = build_summary(cppcheck, repos_scanned, files_scanned, &findings);

    let mut report = Report {
        schema: "haw.plugin.report/1".to_string(),
        plugin: "misra".to_string(),
        phase: opts.phase.clone(),
        ok: true,
        summary,
        artifacts: Vec::new(),
        findings,
    };
    report.recompute_ok();

    Ok(Outcome {
        report,
        files_scanned,
        repos_scanned,
        cppcheck,
    })
}

/// Resolve which repos to scan from the context and optional `--repo`.
fn select_targets(ctx: &context::Context, repo: Option<&str>) -> Vec<Target> {
    match repo {
        Some(explicit) => {
            let path = PathBuf::from(explicit);
            let name = ctx
                .repos
                .iter()
                .find(|r| r.path == path)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| explicit.to_string());
            vec![Target { name, path }]
        }
        None => ctx
            .repos
            .iter()
            .map(|r| Target {
                name: r.name.clone(),
                path: r.path.clone(),
            })
            .collect(),
    }
}

/// Build the one-line human summary carried in the report.
fn build_summary(cppcheck: bool, repos: usize, files: usize, findings: &[Finding]) -> String {
    if !cppcheck {
        return "cppcheck not found; MISRA gate skipped".to_string();
    }
    let violations = findings.iter().filter(|f| f.level == "error").count();
    format!(
        "MISRA: scanned {files} C/C++ file(s) across {repos} repo(s): {violations} violation(s)"
    )
}

/// Render the human-readable summary for the `haw misra` subcommand.
fn human_summary(outcome: &Outcome) -> String {
    let mut out = String::new();
    out.push_str("haw-misra — MISRA C gate\n");
    if !outcome.cppcheck {
        out.push_str("  cppcheck: not found on PATH — gate skipped (fail-open)\n");
        out.push_str("  install cppcheck to enable the MISRA pass.\n");
        return out;
    }
    let violations = outcome
        .report
        .findings
        .iter()
        .filter(|f| f.level == "error")
        .count();
    out.push_str(&format!("  repos scanned:   {}\n", outcome.repos_scanned));
    out.push_str(&format!("  C/C++ files:     {}\n", outcome.files_scanned));
    out.push_str(&format!("  violations:      {violations}\n"));
    out.push_str(&format!(
        "  result:          {}\n",
        if outcome.report.ok { "PASS" } else { "BLOCK" }
    ));
    for finding in &outcome.report.findings {
        if finding.level != "info" {
            out.push_str(&format!("  [{}] {}\n", finding.level, finding.message));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn ctx_json(root: &str, repos: &[(&str, &str)]) -> String {
        let repos: Vec<serde_json::Value> = repos
            .iter()
            .map(|(name, path)| serde_json::json!({"name": name, "path": path}))
            .collect();
        serde_json::json!({
            "schema": "haw.plugin/1",
            "root": root,
            "repos": repos,
        })
        .to_string()
    }

    #[test]
    fn context_parses_root_and_repos() {
        let raw = ctx_json("/ws", &[("hal", "/ws/hal"), ("app", "/ws/app")]);
        let ctx = context::Context::from_json(&raw).unwrap();
        assert_eq!(ctx.root, Some(PathBuf::from("/ws")));
        assert_eq!(ctx.repos.len(), 2);
        assert_eq!(ctx.repos[0].name, "hal");
        assert_eq!(ctx.repos[1].path, PathBuf::from("/ws/app"));
    }

    #[test]
    fn context_degrades_when_repos_absent() {
        let raw = serde_json::json!({"schema": "haw.plugin/1"}).to_string();
        let ctx = context::Context::from_json(&raw).unwrap();
        assert!(ctx.repos.is_empty());
        assert_eq!(ctx.root, None);
    }

    #[test]
    fn context_rejects_wrong_schema() {
        let raw = serde_json::json!({"schema": "nope/9"}).to_string();
        assert!(context::Context::from_json(&raw).is_err());
    }

    #[test]
    fn cppcheck_absent_fail_open_is_ok_true_with_warn() {
        // Simulate the fail-open path directly (no dependency on the host).
        let mut findings = vec![Finding::warn("cppcheck not found; MISRA gate skipped")];
        let summary = build_summary(false, 2, 0, &findings);
        let mut report = Report {
            schema: "haw.plugin.report/1".to_string(),
            plugin: "misra".to_string(),
            phase: Some("pre-request".to_string()),
            ok: true,
            summary,
            artifacts: Vec::new(),
            findings: std::mem::take(&mut findings),
        };
        report.recompute_ok();

        assert!(report.ok, "missing cppcheck must not block");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].level, "warn");
        assert!(
            report.findings[0].message.contains("cppcheck not found"),
            "expected the skip notice"
        );
        assert!(report.summary.contains("skipped"));
    }

    #[test]
    fn report_round_trips_as_plugin_report_schema() {
        let mut report = Report {
            schema: "haw.plugin.report/1".to_string(),
            plugin: "misra".to_string(),
            phase: Some("pre-request".to_string()),
            ok: true,
            summary: "MISRA: scanned 3 C/C++ file(s) across 1 repo(s): 0 violation(s)".to_string(),
            artifacts: Vec::new(),
            findings: vec![Finding::info(
                "cppcheck/misra: no violations in repo 'hal' (3 file(s))",
            )],
        };
        report.recompute_ok();

        let json = serde_json::to_string(&report).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema"], "haw.plugin.report/1");
        assert_eq!(value["plugin"], "misra");
        assert_eq!(value["phase"], "pre-request");
        assert_eq!(value["ok"], true);
        assert_eq!(value["findings"][0]["level"], "info");
        assert!(value["artifacts"].as_array().unwrap().is_empty());
    }

    #[test]
    fn violations_make_the_gate_block() {
        let findings = vec![
            Finding::error("MISRA violation in 'hal': hal.c:12: misra-c2012-15.5: ..."),
            Finding::error("MISRA violation in 'hal': hal.c:40: misra-c2012-17.7: ..."),
        ];
        let summary = build_summary(true, 1, 2, &findings);
        let mut report = Report {
            schema: "haw.plugin.report/1".to_string(),
            plugin: "misra".to_string(),
            phase: Some("pre-request".to_string()),
            ok: true,
            summary,
            artifacts: Vec::new(),
            findings,
        };
        report.recompute_ok();

        assert!(!report.ok, "violations must block the PR");
        assert!(report.summary.contains("2 violation(s)"));
    }

    #[test]
    fn parse_violations_ignores_noise() {
        let stderr = "\
hal.c:12: misra-c2012-15.5: return statement before end of function
hal.c:40: misra-c2012-17.7: value returned is not used

Checking hal.c ...
";
        let v = scan::parse_violations(stderr);
        assert_eq!(v.len(), 2, "only the two colon-bearing diagnostics count");
        assert!(v[0].contains("misra-c2012-15.5"));
    }

    #[test]
    fn collect_c_files_on_missing_path_is_empty() {
        let files = scan::collect_c_files(std::path::Path::new("/definitely/missing/xyz"));
        assert!(files.is_empty());
    }
}
