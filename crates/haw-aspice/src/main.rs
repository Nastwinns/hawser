//! `haw-aspice` — generate ASPICE/qualification traceability from the pinned fleet.
//!
//! Reads a `haw.plugin/1` context (from `HAW_JSON`, falling back to stdin) and
//! produces an Automotive SPICE-style traceability bundle: `aspice-trace.json`
//! (machine, schema `haw.aspice/1`) and `aspice-trace.md` (human report mapping
//! each repo -> pinned SHA -> group onto ASPICE process areas). It enriches
//! pinned SHAs by shelling out to `haw status --format json` when `haw` is on
//! PATH, and tolerates its absence.
//!
//! Two modes:
//! - **Subcommand** (`haw aspice`): writes the bundle and prints the report
//!   path plus a short summary; `--format json` prints the `haw.aspice/1`
//!   document to stdout instead.
//! - **Hook** (`--haw-phase <phase>`, e.g. `post-land`): writes the bundle and
//!   emits a single `haw.plugin.report/1` document (haw-core's [`Report`]) to
//!   stdout, exiting non-zero only on a real failure.

mod cli;
mod context;
mod trace;

use std::path::Path;
use std::process::ExitCode;

use haw_core::plugin::{Artifact, REPORT_SCHEMA, Report};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match cli::parse(&args) {
        Ok(cli::ParseOutcome::Help(text)) => {
            print!("{text}");
            return ExitCode::SUCCESS;
        }
        Ok(cli::ParseOutcome::Run(opts)) => opts,
        Err(err) => {
            eprintln!("haw-aspice: {err}");
            return ExitCode::FAILURE;
        }
    };

    match run(opts) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("haw-aspice: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Run the plugin end to end, dispatching on the requested mode.
fn run(opts: cli::Options) -> Result<ExitCode, String> {
    let raw = context::read_raw_context();
    let ctx = context::Context::from_json(&raw)?;

    let timestamp = trace::resolve_timestamp(opts.at.as_deref());
    let enrich = trace::enrich_from_haw(ctx.root.as_deref());
    let json_doc = trace::build_json(&ctx, &enrich, timestamp.as_deref());
    let markdown = trace::build_markdown(&ctx, &enrich, timestamp.as_deref());

    // Subcommand `--format json` short-circuits: just print the machine doc.
    if opts.json && opts.phase.is_none() {
        let text = serde_json::to_string_pretty(&json_doc)
            .map_err(|e| format!("failed to serialize aspice document: {e}"))?;
        println!("{text}");
        return Ok(ExitCode::SUCCESS);
    }

    // Otherwise write the two artifacts to disk.
    let out_dir = trace::resolve_out_dir(opts.out_dir.as_deref(), &ctx);
    let (json_path, md_path) = write_artifacts(&out_dir, &json_doc, &markdown)?;

    match opts.phase {
        // Hook mode: emit a haw.plugin.report/1 document.
        Some(phase) => {
            let report = Report {
                schema: REPORT_SCHEMA.to_string(),
                plugin: "aspice".to_string(),
                phase: Some(phase),
                ok: true,
                summary: trace::summary(&ctx),
                artifacts: vec![
                    Artifact {
                        path: json_path.to_string_lossy().into_owned(),
                        kind: "report".to_string(),
                    },
                    Artifact {
                        path: md_path.to_string_lossy().into_owned(),
                        kind: "report".to_string(),
                    },
                ],
                findings: Vec::new(),
            };
            let text = serde_json::to_string(&report)
                .map_err(|e| format!("failed to serialize report: {e}"))?;
            println!("{text}");
            Ok(ExitCode::SUCCESS)
        }
        // Subcommand mode: human-friendly output.
        None => {
            println!("{}", trace::summary(&ctx));
            println!("wrote {}", md_path.display());
            println!("wrote {}", json_path.display());
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Write the JSON and markdown artifacts, returning their paths.
fn write_artifacts(
    out_dir: &Path,
    json_doc: &serde_json::Value,
    markdown: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("failed to create {}: {e}", out_dir.display()))?;

    let json_path = out_dir.join(trace::JSON_ARTIFACT);
    let md_path = out_dir.join(trace::MD_ARTIFACT);

    let json_text = serde_json::to_string_pretty(json_doc)
        .map_err(|e| format!("failed to serialize aspice document: {e}"))?;
    std::fs::write(&json_path, format!("{json_text}\n"))
        .map_err(|e| format!("failed to write {}: {e}", json_path.display()))?;
    std::fs::write(&md_path, markdown)
        .map_err(|e| format!("failed to write {}: {e}", md_path.display()))?;

    Ok((json_path, md_path))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn hook_report_serializes_against_report_schema() {
        let report = Report {
            schema: REPORT_SCHEMA.to_string(),
            plugin: "aspice".to_string(),
            phase: Some("post-land".to_string()),
            ok: true,
            summary: "aspice: traced 2 repos @ pinned SHAs".to_string(),
            artifacts: vec![Artifact {
                path: "/ws/aspice-trace.json".to_string(),
                kind: "report".to_string(),
            }],
            findings: Vec::new(),
        };
        let json = serde_json::to_string(&report).unwrap();
        // Round-trips back through haw-core's own parser.
        let parsed = Report::parse(&json).unwrap();
        assert_eq!(parsed.schema, REPORT_SCHEMA);
        assert_eq!(parsed.plugin, "aspice");
        assert_eq!(parsed.artifacts[0].kind, "report");
        assert!(parsed.ok);
    }

    #[test]
    fn write_artifacts_creates_both_files() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = serde_json::json!({ "schema": "haw.aspice/1" });
        let (json_path, md_path) = write_artifacts(tmp.path(), &doc, "# trace\n").unwrap();
        assert!(json_path.exists());
        assert!(md_path.exists());
        let md = std::fs::read_to_string(&md_path).unwrap();
        assert_eq!(md, "# trace\n");
    }
}
