//! The `haw.plugin.report/1` output structures, serialized with raw `serde`.
//!
//! This mirrors `haw-core`'s `plugin::{Report, Finding, Artifact}` on the wire
//! (schema tag `haw.plugin.report/1`) so haw parses it with the same code path,
//! but is kept local so the plugin depends only on `serde`/`serde_json`.

use serde::Serialize;

/// A single finding emitted by the gate.
#[derive(Serialize)]
pub struct Finding {
    /// The severity: `info`, `warn`, or `error`.
    pub level: String,
    /// A human-readable description of the finding.
    pub message: String,
}

impl Finding {
    /// Build an `info`-level finding.
    pub fn info(message: impl Into<String>) -> Self {
        Finding {
            level: "info".to_string(),
            message: message.into(),
        }
    }

    /// Build a `warn`-level finding.
    pub fn warn(message: impl Into<String>) -> Self {
        Finding {
            level: "warn".to_string(),
            message: message.into(),
        }
    }

    /// Build an `error`-level finding.
    pub fn error(message: impl Into<String>) -> Self {
        Finding {
            level: "error".to_string(),
            message: message.into(),
        }
    }
}

/// The full `haw.plugin.report/1` report.
#[derive(Serialize)]
pub struct Report {
    /// The report schema identifier.
    pub schema: String,
    /// The emitting plugin name.
    pub plugin: String,
    /// The phase this run was invoked for, if any.
    pub phase: Option<String>,
    /// Whether the gate passed (no `error`-level findings).
    pub ok: bool,
    /// A one-line human summary.
    pub summary: String,
    /// Any artifacts produced (this plugin produces none).
    pub artifacts: Vec<serde_json::Value>,
    /// The findings, in emission order.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Recompute `ok` from the current findings: true unless any `error` exists.
    pub fn recompute_ok(&mut self) {
        self.ok = !self.findings.iter().any(|f| f.level == "error");
    }
}
