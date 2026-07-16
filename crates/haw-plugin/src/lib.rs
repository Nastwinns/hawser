//! SDK for authoring `haw-<name>` plugins.
//!
//! A plugin is a standalone executable haw dispatches out-of-process (see
//! `docs/PLUGINS.md`). haw passes the `haw.plugin/1` context on the `HAW_JSON`
//! env var (falling back to stdin) and the lifecycle phase as `--haw-phase
//! <phase>`. The plugin prints a `haw.plugin.report/1` report to stdout.
//!
//! This crate is OPTIONAL sugar for writing that binary in Rust; haw-core does
//! not depend on it. A minimal plugin:
//!
//! ```no_run
//! use haw_plugin::{run, Report};
//!
//! fn main() {
//!     run("sbom", |ctx, phase| {
//!         Report::new("sbom", phase)
//!             .ok(true)
//!             .summary(format!("scanned {} repo(s)", ctx.repos.len()))
//!             .artifact("sbom.json", "sbom")
//!             .finding("info", "no vulnerabilities")
//!     });
//! }
//! ```

use std::io::Read;

use serde::{Deserialize, Serialize};

/// The wire-contract version this SDK targets.
pub const CONTRACT: &str = "haw.plugin/1";

/// The report-schema version plugins emit on stdout.
pub const REPORT_SCHEMA: &str = "haw.plugin.report/1";

/// The `haw.plugin/1` context haw hands a plugin.
///
/// Outside a workspace only `schema` is present; `root`, `stack`, and `repos`
/// are absent and deserialize to their empty defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Context {
    /// Schema tag; equals [`CONTRACT`].
    pub schema: String,
    /// The lifecycle phase, when dispatched via `[plugins]`.
    #[serde(default)]
    pub phase: Option<String>,
    /// Absolute workspace root, when inside a workspace.
    #[serde(default)]
    pub root: Option<String>,
    /// The active stack, if any.
    #[serde(default)]
    pub stack: Option<String>,
    /// The workspace's repos.
    #[serde(default)]
    pub repos: Vec<Repo>,
}

/// One repo entry in the context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    /// Absolute checkout path.
    pub path: String,
    /// The repo's pinned revision.
    pub rev: String,
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Errors reading the plugin context.
#[derive(Debug)]
pub enum ContextError {
    /// Neither `HAW_JSON` nor stdin carried a context.
    Missing,
    /// The context was present but could not be read from stdin.
    Io(std::io::Error),
    /// The context JSON did not match `haw.plugin/1`.
    Parse(serde_json::Error),
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextError::Missing => write!(f, "no haw.plugin/1 context on HAW_JSON or stdin"),
            ContextError::Io(err) => write!(f, "reading context from stdin: {err}"),
            ContextError::Parse(err) => write!(f, "parsing haw.plugin/1 context: {err}"),
        }
    }
}

impl std::error::Error for ContextError {}

impl Context {
    /// Read the context from the `HAW_JSON` env var, falling back to stdin.
    pub fn load() -> Result<Context, ContextError> {
        let raw = match std::env::var("HAW_JSON") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => {
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(ContextError::Io)?;
                if buf.trim().is_empty() {
                    return Err(ContextError::Missing);
                }
                buf
            }
        };
        Self::from_json(&raw)
    }

    /// Parse a context from a JSON string.
    pub fn from_json(raw: &str) -> Result<Context, ContextError> {
        serde_json::from_str(raw.trim()).map_err(ContextError::Parse)
    }
}

/// A `haw.plugin.report/1` document a plugin emits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Schema tag; equals [`REPORT_SCHEMA`].
    pub schema: String,
    pub plugin: String,
    #[serde(default)]
    pub phase: Option<String>,
    pub ok: bool,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

/// One artifact a plugin produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    /// One of `sbom`, `signature`, `provenance`, `log`, `report`.
    pub kind: String,
}

/// One finding a plugin surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// One of `info`, `warn`, `error`.
    pub level: String,
    pub message: String,
}

impl Report {
    /// Start a report for `plugin` and `phase`, defaulting to `ok: true`.
    pub fn new(plugin: impl Into<String>, phase: Option<&str>) -> Report {
        Report {
            schema: REPORT_SCHEMA.to_string(),
            plugin: plugin.into(),
            phase: phase.map(str::to_string),
            ok: true,
            summary: String::new(),
            artifacts: Vec::new(),
            findings: Vec::new(),
        }
    }

    /// Set the pass/fail flag.
    pub fn ok(mut self, ok: bool) -> Report {
        self.ok = ok;
        self
    }

    /// Set the human summary line.
    pub fn summary(mut self, summary: impl Into<String>) -> Report {
        self.summary = summary.into();
        self
    }

    /// Add an artifact (`kind` is one of `sbom`, `signature`, `provenance`,
    /// `log`, `report`).
    pub fn artifact(mut self, path: impl Into<String>, kind: impl Into<String>) -> Report {
        self.artifacts.push(Artifact {
            path: path.into(),
            kind: kind.into(),
        });
        self
    }

    /// Add a finding (`level` is one of `info`, `warn`, `error`).
    pub fn finding(mut self, level: impl Into<String>, message: impl Into<String>) -> Report {
        self.findings.push(Finding {
            level: level.into(),
            message: message.into(),
        });
        self
    }

    /// Serialize to the `haw.plugin.report/1` JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                "{{\"schema\":\"{REPORT_SCHEMA}\",\"plugin\":\"{}\",\"ok\":false,\"summary\":\"report serialization failed\"}}",
                self.plugin
            )
        })
    }

    /// Print the report to stdout as a single line.
    pub fn emit(&self) {
        println!("{}", self.to_json());
    }
}

/// Read the `--haw-phase <name>` argument from an argv slice, if present.
pub fn phase_arg(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--haw-phase" {
            return it.next().cloned();
        }
        if let Some(rest) = arg.strip_prefix("--haw-phase=") {
            return Some(rest.to_string());
        }
    }
    None
}

/// Entry helper for a `haw-<name>` binary.
///
/// Loads the context, reads the `--haw-phase` argument, runs `body`, prints the
/// resulting report, and exits with status `0` when `ok` is `true`, else `1`.
/// If the context cannot be loaded, prints a failing report and exits `1`.
pub fn run<F>(plugin: &str, body: F) -> !
where
    F: FnOnce(&Context, Option<&str>) -> Report,
{
    let args: Vec<String> = std::env::args().skip(1).collect();
    let phase = phase_arg(&args);
    let report = match Context::load() {
        Ok(ctx) => body(&ctx, phase.as_deref()),
        Err(err) => Report::new(plugin, phase.as_deref())
            .ok(false)
            .summary(err.to_string()),
    };
    report.emit();
    std::process::exit(if report.ok { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn context_round_trips() {
        let json = serde_json::json!({
            "schema": CONTRACT,
            "phase": "post-build",
            "root": "/ws",
            "stack": "prod",
            "repos": [{
                "name": "api",
                "path": "/ws/api",
                "rev": "abc123",
                "groups": ["core"],
            }],
        })
        .to_string();

        let ctx = Context::from_json(&json).expect("parse");
        assert_eq!(ctx.schema, CONTRACT);
        assert_eq!(ctx.phase.as_deref(), Some("post-build"));
        assert_eq!(ctx.root.as_deref(), Some("/ws"));
        assert_eq!(ctx.repos.len(), 1);
        assert_eq!(ctx.repos[0].name, "api");
        assert_eq!(ctx.repos[0].groups, vec!["core".to_string()]);

        let back = serde_json::to_string(&ctx).unwrap();
        let reparsed = Context::from_json(&back).unwrap();
        assert_eq!(ctx, reparsed);
    }

    #[test]
    fn context_outside_workspace() {
        let ctx = Context::from_json(r#"{"schema":"haw.plugin/1"}"#).unwrap();
        assert_eq!(ctx.schema, CONTRACT);
        assert!(ctx.root.is_none());
        assert!(ctx.repos.is_empty());
    }

    #[test]
    fn report_builds_and_round_trips() {
        let report = Report::new("sbom", Some("post-build"))
            .ok(true)
            .summary("scanned 3 repos")
            .artifact("sbom.json", "sbom")
            .finding("info", "clean");

        let json = report.to_json();
        let reparsed: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(report, reparsed);
        assert_eq!(reparsed.schema, REPORT_SCHEMA);
        assert!(reparsed.ok);
        assert_eq!(reparsed.artifacts[0].kind, "sbom");
        assert_eq!(reparsed.findings[0].level, "info");
    }

    #[test]
    fn phase_arg_parsing() {
        assert_eq!(
            phase_arg(&["--haw-phase".to_string(), "pre-test".to_string()]),
            Some("pre-test".to_string())
        );
        assert_eq!(
            phase_arg(&["--haw-phase=post-land".to_string()]),
            Some("post-land".to_string())
        );
        assert_eq!(phase_arg(&["--other".to_string()]), None);
    }
}
