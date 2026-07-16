//! `haw-jira` — link a haw changeset to a Jira issue and transition it as the
//! change lands.
//!
//! Reads a `haw.plugin/1` context (from `HAW_JSON`, falling back to stdin). The
//! issue key comes from an explicit argv arg, else the current `change/<ID>`
//! changeset (from the context or `haw change status --format json`). Config is
//! read from `JIRA_URL`/`JIRA_USER`/`JIRA_TOKEN`; when any is missing it runs a
//! **dry-run** (prints the action it would take, exits 0). With creds present it
//! comments on the issue and transitions it via the Jira REST API.
//!
//! Two modes:
//! - **Subcommand** (`haw jira PROJ-123`): human output; `--format json` prints
//!   the planned/performed action.
//! - **Hook** (`--haw-phase <phase>`): emits a `haw.plugin.report/1` document
//!   (haw-core's [`Report`]); non-zero exit only on a real failure.

mod cli;
mod context;
mod jira;

use std::process::ExitCode;

use haw_core::plugin::{Finding, REPORT_SCHEMA, Report};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match cli::parse(&args) {
        Ok(cli::ParseOutcome::Help(text)) => {
            print!("{text}");
            return ExitCode::SUCCESS;
        }
        Ok(cli::ParseOutcome::Run(opts)) => opts,
        Err(err) => {
            eprintln!("haw-jira: {err}");
            return ExitCode::FAILURE;
        }
    };

    match run(opts) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("haw-jira: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Run the plugin end to end, dispatching on the requested mode.
fn run(opts: cli::Options) -> Result<ExitCode, String> {
    let raw = context::read_raw_context();
    let ctx = context::Context::from_json(&raw)?;

    let issue = jira::resolve_issue(opts.issue.as_deref(), &ctx)?;
    let phase = opts.phase.clone().or_else(|| ctx.phase.clone());
    let target = opts
        .to
        .clone()
        .unwrap_or_else(|| jira::default_target(phase.as_deref()).to_string());

    let action = jira::plan(&ctx, issue, target);

    // Fail-open: no creds -> dry-run, always ok.
    let config = jira::Config::from_env();
    match config {
        None => emit_dry_run(&opts, phase.as_deref(), action),
        Some(config) => match jira::perform(&config, action) {
            Ok(performed) => emit_success(&opts, phase.as_deref(), performed),
            Err(err) => emit_failure(&opts, phase.as_deref(), err),
        },
    }
}

/// Dry-run path: print the planned action; exit 0 with ok:true.
fn emit_dry_run(
    opts: &cli::Options,
    phase: Option<&str>,
    action: jira::Action,
) -> Result<ExitCode, String> {
    if opts.json && phase.is_none() {
        print_action_json(&action)?;
        return Ok(ExitCode::SUCCESS);
    }

    let summary = format!(
        "jira (dry-run): would transition {} to '{}' and comment",
        action.issue, action.target_status
    );

    if let Some(phase) = phase {
        let report = base_report(phase, true, summary, Vec::new());
        print_report(&report)?;
    } else {
        println!("{summary}");
        println!("  comment: {}", action.comment);
        println!("  (set JIRA_URL/JIRA_USER/JIRA_TOKEN to perform for real)");
    }
    Ok(ExitCode::SUCCESS)
}

/// Success path after performing the live action.
fn emit_success(
    opts: &cli::Options,
    phase: Option<&str>,
    performed: jira::Performed,
) -> Result<ExitCode, String> {
    if opts.json && phase.is_none() {
        print_action_json(&performed.action)?;
        return Ok(ExitCode::SUCCESS);
    }

    let summary = format!(
        "jira: transitioned {} to '{}'",
        performed.action.issue, performed.action.target_status
    );

    if let Some(phase) = phase {
        let findings = performed
            .notes
            .iter()
            .map(|n| Finding {
                level: "info".to_string(),
                message: n.clone(),
            })
            .collect();
        let report = base_report(phase, true, summary, findings);
        print_report(&report)?;
    } else {
        println!("{summary}");
        for note in &performed.notes {
            println!("  {note}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Failure path: report the error; non-zero exit only in hook mode.
fn emit_failure(opts: &cli::Options, phase: Option<&str>, err: String) -> Result<ExitCode, String> {
    if let Some(phase) = phase {
        let findings = vec![Finding {
            level: "error".to_string(),
            message: err.clone(),
        }];
        let report = base_report(phase, false, format!("jira: {err}"), findings);
        print_report(&report)?;
        // Hook mode: a real failure is a non-zero exit.
        return Ok(ExitCode::FAILURE);
    }

    if opts.json {
        let doc = serde_json::json!({ "schema": "haw.jira/1", "error": err });
        let text = serde_json::to_string_pretty(&doc)
            .map_err(|e| format!("failed to serialize error: {e}"))?;
        println!("{text}");
    } else {
        eprintln!("haw-jira: {err}");
    }
    // Subcommand mode: surface as a plain error exit.
    Ok(ExitCode::FAILURE)
}

/// Build a `haw.plugin.report/1` report for the given phase.
fn base_report(phase: &str, ok: bool, summary: String, findings: Vec<Finding>) -> Report {
    Report {
        schema: REPORT_SCHEMA.to_string(),
        plugin: "jira".to_string(),
        phase: Some(phase.to_string()),
        ok,
        summary,
        artifacts: Vec::new(),
        findings,
    }
}

fn print_report(report: &Report) -> Result<(), String> {
    let text =
        serde_json::to_string(report).map_err(|e| format!("failed to serialize report: {e}"))?;
    println!("{text}");
    Ok(())
}

fn print_action_json(action: &jira::Action) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&action.to_json())
        .map_err(|e| format!("failed to serialize action: {e}"))?;
    println!("{text}");
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn hook_report_round_trips_against_report_schema() {
        let report = base_report(
            "post-land",
            true,
            "jira: transitioned PROJ-42 to 'Done'".to_string(),
            vec![Finding {
                level: "info".to_string(),
                message: "commented on PROJ-42".to_string(),
            }],
        );
        let json = serde_json::to_string(&report).unwrap();
        let parsed = Report::parse(&json).unwrap();
        assert_eq!(parsed.schema, REPORT_SCHEMA);
        assert_eq!(parsed.plugin, "jira");
        assert_eq!(parsed.phase.as_deref(), Some("post-land"));
        assert!(parsed.ok);
    }

    #[test]
    fn failure_report_is_not_ok_and_carries_error_finding() {
        let report = base_report(
            "post-land",
            false,
            "jira: boom".to_string(),
            vec![Finding {
                level: "error".to_string(),
                message: "boom".to_string(),
            }],
        );
        let json = serde_json::to_string(&report).unwrap();
        let parsed = Report::parse(&json).unwrap();
        assert!(!parsed.ok);
        assert_eq!(parsed.findings[0].level, "error");
    }
}
