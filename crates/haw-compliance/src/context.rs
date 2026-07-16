//! Reading and modeling the `haw.plugin/1` input context.
//!
//! The context is parsed from raw JSON (via `serde_json`) rather than the
//! `haw-plugin` SDK, so this plugin does not depend on that crate's evolving
//! API.

use std::io::Read;

use serde::Deserialize;

/// A single repository entry from the `haw.plugin/1` context.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Repo {
    /// The repository's logical name.
    pub name: String,
    /// The absolute path to the repository on disk.
    #[serde(default)]
    pub path: Option<String>,
    /// The pinned revision (commit SHA) for this repository.
    #[serde(default)]
    pub rev: Option<String>,
    /// The group memberships for this repository.
    #[serde(default)]
    pub groups: Vec<String>,
}

/// The parsed `haw.plugin/1` context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// The workspace root (absolute path), if inside a workspace.
    pub root: Option<String>,
    /// The active stack name, if any.
    pub stack: Option<String>,
    /// The repositories in dependency composition, sorted stably by name then rev.
    pub repos: Vec<Repo>,
}

/// Raw shape used purely for deserialization before normalization.
#[derive(Debug, Deserialize)]
struct RawContext {
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    root: Option<String>,
    #[serde(default)]
    stack: Option<String>,
    #[serde(default)]
    repos: Vec<Repo>,
}

impl Context {
    /// Parse a `haw.plugin/1` context from a raw JSON string.
    ///
    /// Repositories are sorted stably by `(name, rev)` so that the resulting
    /// SBOM component ordering is deterministic regardless of input order.
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let parsed: RawContext =
            serde_json::from_str(raw).map_err(|e| format!("invalid haw.plugin/1 JSON: {e}"))?;

        if let Some(schema) = &parsed.schema
            && schema != "haw.plugin/1"
        {
            return Err(format!("unexpected schema: {schema} (want haw.plugin/1)"));
        }

        let mut repos = parsed.repos;
        repos.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.rev.cmp(&b.rev)));

        Ok(Context {
            root: parsed.root,
            stack: parsed.stack,
            repos,
        })
    }
}

/// Read the raw context JSON: from `HAW_JSON` if set, otherwise from stdin.
///
/// If neither source yields any bytes, an empty `haw.plugin/1` document is
/// returned so that the plugin degrades gracefully outside a workspace.
pub fn read_raw_context() -> Result<String, String> {
    if let Ok(value) = std::env::var("HAW_JSON")
        && !value.trim().is_empty()
    {
        return Ok(value);
    }

    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("failed to read context from stdin: {e}"))?;

    if buf.trim().is_empty() {
        return Ok(r#"{"schema":"haw.plugin/1"}"#.to_string());
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_repos_and_sorts() {
        let raw = r#"{
            "schema": "haw.plugin/1",
            "root": "/ws",
            "stack": "main",
            "repos": [
                {"name": "zebra", "path": "/ws/z", "rev": "ffff", "groups": ["g1"]},
                {"name": "alpha", "path": "/ws/a", "rev": "aaaa", "groups": []}
            ]
        }"#;
        let ctx = Context::from_json(raw).unwrap();
        assert_eq!(ctx.root.as_deref(), Some("/ws"));
        assert_eq!(ctx.stack.as_deref(), Some("main"));
        assert_eq!(ctx.repos.len(), 2);
        assert_eq!(ctx.repos[0].name, "alpha");
        assert_eq!(ctx.repos[1].name, "zebra");
    }

    #[test]
    fn parses_empty_context() {
        let ctx = Context::from_json(r#"{"schema":"haw.plugin/1"}"#).unwrap();
        assert!(ctx.root.is_none());
        assert!(ctx.repos.is_empty());
    }

    #[test]
    fn rejects_bad_schema() {
        assert!(Context::from_json(r#"{"schema":"other/1"}"#).is_err());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(Context::from_json("not json").is_err());
    }
}
