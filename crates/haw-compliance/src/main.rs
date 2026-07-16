//! `haw-compliance` — a haw plugin binary that generates a composition-level SBOM.
//!
//! Reads a `haw.plugin/1` context (from the `HAW_JSON` env var, falling back to
//! stdin), builds a composition-level SBOM listing every repo as a component
//! (name, version = pinned `rev`, a purl-like id and a supplier), and emits both
//! a CycloneDX 1.5 JSON and an SPDX 2.3 JSON document with the NTIA-minimum
//! fields. It writes `sbom.cdx.json` and `sbom.spdx.json` to the out dir and
//! prints a `haw.plugin.report/1` report to stdout.

// Test code is permitted to use `unwrap`/`expect`/`panic` per project rules.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod cli;
mod context;
mod report;
mod sbom;

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match cli::parse(&args) {
        Ok(cli::ParseOutcome::Help(text)) => {
            print!("{text}");
            return ExitCode::SUCCESS;
        }
        Ok(cli::ParseOutcome::Run(opts)) => opts,
        Err(err) => {
            eprintln!("haw-compliance: {err}");
            return ExitCode::FAILURE;
        }
    };

    match run(opts) {
        Ok(report) => {
            match serde_json::to_string(&report) {
                Ok(json) => println!("{json}"),
                Err(err) => {
                    eprintln!("haw-compliance: failed to serialize report: {err}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("haw-compliance: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Run the plugin end to end, returning the report to be printed to stdout.
fn run(opts: cli::Options) -> Result<report::Report, String> {
    let raw = context::read_raw_context()?;
    let ctx = context::Context::from_json(&raw)?;

    let out_dir = opts.resolve_out_dir(ctx.root.as_deref());
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create out dir {}: {e}", out_dir.display()))?;

    let timestamp = sbom::deterministic_timestamp();

    let mut findings: Vec<report::Finding> = Vec::new();

    match sbom::detect_scanner() {
        Some(scanner) => findings.push(report::Finding::info(format!(
            "per-ecosystem scanner found on PATH: {scanner}; composition-level SBOM emitted"
        ))),
        None => findings.push(report::Finding::warn(
            "per-ecosystem scanner not found; composition-level SBOM only".to_string(),
        )),
    }

    let cdx = sbom::cyclonedx_document(&ctx, timestamp.as_deref());
    let spdx = sbom::spdx_document(&ctx, timestamp.as_deref());

    let cdx_path = out_dir.join("sbom.cdx.json");
    let spdx_path = out_dir.join("sbom.spdx.json");

    write_json(&cdx_path, &cdx)?;
    write_json(&spdx_path, &spdx)?;

    let artifacts = vec![
        report::Artifact::sbom(cdx_path.to_string_lossy().into_owned()),
        report::Artifact::sbom(spdx_path.to_string_lossy().into_owned()),
    ];

    let summary = format!(
        "composition-level SBOM generated for {} component(s)",
        ctx.repos.len()
    );

    Ok(report::Report {
        schema: "haw.plugin.report/1".to_string(),
        plugin: "haw-compliance".to_string(),
        phase: opts.phase,
        ok: true,
        summary,
        artifacts,
        findings,
    })
}

/// Serialize `value` as pretty JSON with a trailing newline and write it to `path`.
fn write_json<T: serde::Serialize>(path: &std::path::Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("failed to serialize {}: {e}", path.display()))?;
    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    file.write_all(json.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    Ok(())
}
