//! Out-of-process plugin dispatch.
//!
//! When haw reaches a lifecycle [phase](crate::hooks::Hook), every plugin
//! registered in the manifest's `[plugins]` table that subscribes to that
//! phase is invoked as a standalone `haw-<name>` executable. The
//! `haw.plugin/1` context is passed both on the `HAW_JSON` env var and on
//! stdin; the phase name is passed as `--haw-phase <phase>`. Each plugin
//! prints a [`Report`] (`haw.plugin.report/1`) to stdout, which is parsed and
//! collected here.
//!
//! Dispatch is fail-open: a plugin binary that is missing or unregistered is
//! skipped, not treated as an error. A `pre-*` plugin that returns `ok: false`
//! is surfaced so the caller may abort; `post-*` reports are advisory.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// The wire-contract version haw passes to plugins.
pub const CONTRACT: &str = "haw.plugin/1";

/// The report-schema version plugins emit on stdout.
pub const REPORT_SCHEMA: &str = "haw.plugin.report/1";

/// A parsed `haw.plugin.report/1` document emitted by a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Schema tag; expected to equal [`REPORT_SCHEMA`].
    pub schema: String,
    /// The plugin's own name (`sbom` for `haw-sbom`).
    pub plugin: String,
    /// The lifecycle phase this report is for, if any.
    #[serde(default)]
    pub phase: Option<String>,
    /// Whether the plugin considers the operation acceptable.
    pub ok: bool,
    /// A one-line human summary.
    #[serde(default)]
    pub summary: String,
    /// Artifacts the plugin produced (SBOMs, signatures, logs, …).
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    /// Findings the plugin wants surfaced to the user.
    #[serde(default)]
    pub findings: Vec<Finding>,
}

/// One artifact a plugin emitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    /// One of `sbom`, `signature`, `provenance`, `log`, `report`.
    pub kind: String,
}

/// One finding a plugin surfaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// One of `info`, `warn`, `error`.
    pub level: String,
    pub message: String,
}

impl Report {
    /// Parse a `haw.plugin.report/1` document from a plugin's stdout.
    pub fn parse(stdout: &str) -> Result<Report, PluginError> {
        let report: Report = serde_json::from_str(stdout.trim()).map_err(PluginError::BadReport)?;
        if report.schema != REPORT_SCHEMA {
            return Err(PluginError::WrongSchema { got: report.schema });
        }
        Ok(report)
    }
}

/// Errors that arise while dispatching or parsing a plugin report.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin produced no parseable report")]
    BadReport(#[source] serde_json::Error),
    #[error("report declares unexpected schema `{got}`")]
    WrongSchema { got: String },
}

/// The outcome of dispatching one plugin for one phase.
#[derive(Debug, Clone, PartialEq)]
pub enum Dispatch {
    /// The plugin ran and produced a report.
    Ran(Report),
    /// The `haw-<name>` binary was not found on PATH; skipped (fail-open).
    Missing { plugin: String },
    /// The plugin ran but its stdout was not a valid report.
    Unparseable { plugin: String, detail: String },
}

impl Dispatch {
    /// The report, if the plugin ran and parsed cleanly.
    pub fn report(&self) -> Option<&Report> {
        match self {
            Dispatch::Ran(report) => Some(report),
            _ => None,
        }
    }
}

/// Anything that can spawn a plugin process and hand back its stdout.
///
/// Abstracted so callers can drive dispatch deterministically in tests
/// without spawning real processes.
pub trait PluginRunner {
    /// Run `haw-<plugin> --haw-phase <phase>` with `context` on both the
    /// `HAW_JSON` env var and stdin. Returns `Ok(None)` when the binary is
    /// missing (fail-open), `Ok(Some(stdout))` when it ran.
    fn run(
        &self,
        plugin: &str,
        phase: &str,
        context: &serde_json::Value,
    ) -> std::io::Result<Option<String>>;
}

/// The real runner: spawns `haw-<name>` out of process.
pub struct ProcessRunner;

impl PluginRunner for ProcessRunner {
    fn run(
        &self,
        plugin: &str,
        phase: &str,
        context: &serde_json::Value,
    ) -> std::io::Result<Option<String>> {
        use std::io::Write;

        let binary = format!("haw-{plugin}");
        let body = context.to_string();
        let spawned = Command::new(&binary)
            .arg("--haw-phase")
            .arg(phase)
            .env("HAW_JSON", &body)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }
        let output = child.wait_with_output()?;
        Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
    }
}

/// Build the `haw.plugin/1` context for a phase, tagged with `"phase"`.
///
/// `repos` is the list of `(name, abs_path, rev, groups)` tuples; `root` and
/// `stack` describe the workspace. Mirrors the contract emitted by
/// `hawser`'s `fn plugin`, with the added `"phase"` field.
pub fn phase_context(
    root: &Path,
    stack: Option<&str>,
    repos: &[RepoContext],
    phase: &str,
) -> serde_json::Value {
    serde_json::json!({
        "schema": CONTRACT,
        "phase": phase,
        "root": root.to_string_lossy(),
        "stack": stack,
        "repos": repos.iter().map(|r| serde_json::json!({
            "name": r.name,
            "path": r.path.to_string_lossy(),
            "rev": r.rev,
            "groups": r.groups,
        })).collect::<Vec<_>>(),
    })
}

/// One repo entry for the plugin context.
#[derive(Debug, Clone)]
pub struct RepoContext {
    pub name: String,
    pub path: std::path::PathBuf,
    pub rev: String,
    pub groups: Vec<String>,
}

/// Dispatch every plugin subscribed to `phase` and collect their outcomes.
///
/// `subscriptions` is the manifest's `[plugins]` table. Only plugins that list
/// `phase` are invoked, in manifest order. Unregistered plugins are never run:
/// haw does not auto-discover binaries from PATH.
pub fn dispatch(
    runner: &dyn PluginRunner,
    subscriptions: &indexmap::IndexMap<String, Vec<String>>,
    phase: &str,
    context: &serde_json::Value,
) -> Vec<Dispatch> {
    let mut out = Vec::new();
    for (plugin, phases) in subscriptions {
        if !phases.iter().any(|p| p == phase) {
            continue;
        }
        match runner.run(plugin, phase, context) {
            Ok(None) => out.push(Dispatch::Missing {
                plugin: plugin.clone(),
            }),
            Ok(Some(stdout)) => match Report::parse(&stdout) {
                Ok(report) => out.push(Dispatch::Ran(report)),
                Err(err) => out.push(Dispatch::Unparseable {
                    plugin: plugin.clone(),
                    detail: err.to_string(),
                }),
            },
            Err(err) => out.push(Dispatch::Unparseable {
                plugin: plugin.clone(),
                detail: err.to_string(),
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use indexmap::IndexMap;

    fn sample_stdout(plugin: &str, ok: bool) -> String {
        serde_json::json!({
            "schema": REPORT_SCHEMA,
            "plugin": plugin,
            "phase": "post-build",
            "ok": ok,
            "summary": "did the thing",
            "artifacts": [{"path": "sbom.json", "kind": "sbom"}],
            "findings": [{"level": "info", "message": "all good"}],
        })
        .to_string()
    }

    struct CannedRunner {
        outputs: IndexMap<String, Option<String>>,
    }

    impl PluginRunner for CannedRunner {
        fn run(
            &self,
            plugin: &str,
            _phase: &str,
            _context: &serde_json::Value,
        ) -> std::io::Result<Option<String>> {
            Ok(self.outputs.get(plugin).cloned().flatten())
        }
    }

    #[test]
    fn parses_a_canned_report() {
        let report = Report::parse(&sample_stdout("sbom", true)).expect("parse");
        assert_eq!(report.plugin, "sbom");
        assert!(report.ok);
        assert_eq!(report.artifacts.len(), 1);
        assert_eq!(report.artifacts[0].kind, "sbom");
        assert_eq!(report.findings[0].level, "info");
    }

    #[test]
    fn rejects_wrong_schema() {
        let bad = serde_json::json!({
            "schema": "nope/9", "plugin": "x", "ok": true,
        })
        .to_string();
        assert!(matches!(
            Report::parse(&bad),
            Err(PluginError::WrongSchema { .. })
        ));
    }

    #[test]
    fn dispatch_only_runs_subscribed_plugins() {
        let mut outputs = IndexMap::new();
        outputs.insert("sbom".to_string(), Some(sample_stdout("sbom", true)));
        let runner = CannedRunner { outputs };

        let mut subs = IndexMap::new();
        subs.insert("sbom".to_string(), vec!["post-build".to_string()]);
        subs.insert("sign".to_string(), vec!["post-land".to_string()]);

        let ctx = serde_json::json!({"schema": CONTRACT});
        let results = dispatch(&runner, &subs, "post-build", &ctx);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].report().map(|r| r.plugin.as_str()), Some("sbom"));
    }

    #[test]
    fn missing_binary_is_skipped_fail_open() {
        let outputs: IndexMap<String, Option<String>> =
            [("ghost".to_string(), None)].into_iter().collect();
        let runner = CannedRunner { outputs };

        let mut subs = IndexMap::new();
        subs.insert("ghost".to_string(), vec!["post-build".to_string()]);

        let ctx = serde_json::json!({"schema": CONTRACT});
        let results = dispatch(&runner, &subs, "post-build", &ctx);

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], Dispatch::Missing { .. }));
    }

    #[test]
    fn unregistered_plugin_never_dispatched() {
        let outputs: IndexMap<String, Option<String>> = IndexMap::new();
        let runner = CannedRunner { outputs };
        let subs: IndexMap<String, Vec<String>> = IndexMap::new();
        let ctx = serde_json::json!({"schema": CONTRACT});
        let results = dispatch(&runner, &subs, "post-build", &ctx);
        assert!(results.is_empty());
    }

    #[test]
    fn phase_context_carries_phase_and_repos() {
        let repos = vec![RepoContext {
            name: "api".to_string(),
            path: std::path::PathBuf::from("/ws/api"),
            rev: "abc".to_string(),
            groups: vec!["core".to_string()],
        }];
        let ctx = phase_context(Path::new("/ws"), Some("prod"), &repos, "pre-build");
        assert_eq!(ctx["schema"], CONTRACT);
        assert_eq!(ctx["phase"], "pre-build");
        assert_eq!(ctx["stack"], "prod");
        assert_eq!(ctx["repos"][0]["name"], "api");
        assert_eq!(ctx["repos"][0]["rev"], "abc");
    }
}
