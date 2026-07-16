//! The `haw.plugin.report/1` output model printed to stdout.

use serde::Serialize;

/// A `haw.plugin.report/1` report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// The report schema identifier; always `haw.plugin.report/1`.
    pub schema: String,
    /// The plugin name; always `haw-compliance`.
    pub plugin: String,
    /// The phase name, if one was provided via `--haw-phase`.
    pub phase: Option<String>,
    /// Whether the plugin completed successfully.
    pub ok: bool,
    /// A short human-readable summary.
    pub summary: String,
    /// Artifacts produced by this run.
    pub artifacts: Vec<Artifact>,
    /// Findings surfaced by this run.
    pub findings: Vec<Finding>,
}

/// An artifact produced by the plugin.
#[derive(Debug, Clone, Serialize)]
pub struct Artifact {
    /// The path to the artifact.
    pub path: String,
    /// The artifact kind.
    pub kind: String,
}

impl Artifact {
    /// Construct an SBOM artifact at `path`.
    pub fn sbom(path: String) -> Self {
        Artifact {
            path,
            kind: "sbom".to_string(),
        }
    }
}

/// A finding surfaced by the plugin.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// The severity level: `info`, `warn`, or `error`.
    pub level: String,
    /// The human-readable message.
    pub message: String,
}

impl Finding {
    /// Construct an `info`-level finding.
    pub fn info(message: String) -> Self {
        Finding {
            level: "info".to_string(),
            message,
        }
    }

    /// Construct a `warn`-level finding.
    pub fn warn(message: String) -> Self {
        Finding {
            level: "warn".to_string(),
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_serializes_expected_fields() {
        let report = Report {
            schema: "haw.plugin.report/1".to_string(),
            plugin: "haw-compliance".to_string(),
            phase: None,
            ok: true,
            summary: "s".to_string(),
            artifacts: vec![Artifact::sbom("/tmp/x".to_string())],
            findings: vec![Finding::warn("w".to_string())],
        };
        let value: serde_json::Value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["schema"], "haw.plugin.report/1");
        assert_eq!(value["plugin"], "haw-compliance");
        assert_eq!(value["ok"], true);
        assert_eq!(value["artifacts"][0]["kind"], "sbom");
        assert_eq!(value["findings"][0]["level"], "warn");
    }
}
