//! Reads and parses the `haw.plugin/1` input context.
//!
//! Parsing is defensive: run outside a workspace the context degrades to
//! `{ "schema": "haw.plugin/1" }`, so every field beyond `schema` is optional
//! and must never panic when absent.

use std::path::PathBuf;

use haw_core::plugin::CONTRACT;

/// A single repository entry from the `haw.plugin/1` context.
#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
    /// The pinned revision — may be a branch such as `change/<ID>`.
    pub rev: String,
    pub groups: Vec<String>,
}

/// The parsed `haw.plugin/1` context relevant to this plugin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context {
    pub root: Option<PathBuf>,
    pub stack: Option<String>,
    pub phase: Option<String>,
    pub repos: Vec<Repo>,
}

/// Read the raw context JSON from `HAW_JSON`, falling back to stdin.
pub fn read_raw_context() -> String {
    if let Ok(value) = std::env::var("HAW_JSON")
        && !value.trim().is_empty()
    {
        return value;
    }
    use std::io::Read;
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        return String::new();
    }
    buf
}

impl Context {
    /// Parse a `haw.plugin/1` context from a raw JSON string.
    pub fn from_json(raw: &str) -> Result<Self, String> {
        if raw.trim().is_empty() {
            return Ok(Context::default());
        }

        let value: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| format!("invalid context JSON: {e}"))?;

        let schema = value.get("schema").and_then(|s| s.as_str());
        if schema != Some(CONTRACT) {
            return Err(format!(
                "unexpected context schema: {}",
                schema.unwrap_or("<missing>")
            ));
        }

        let root = value
            .get("root")
            .and_then(|r| r.as_str())
            .map(PathBuf::from);
        let stack = value
            .get("stack")
            .and_then(|s| s.as_str())
            .map(str::to_string);
        let phase = value
            .get("phase")
            .and_then(|p| p.as_str())
            .map(str::to_string);

        let mut repos = Vec::new();
        if let Some(arr) = value.get("repos").and_then(|r| r.as_array()) {
            for entry in arr {
                let path = match entry.get("path").and_then(|p| p.as_str()) {
                    Some(p) => PathBuf::from(p),
                    None => continue,
                };
                let name = entry
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("<unnamed>")
                    .to_string();
                let rev = entry
                    .get("rev")
                    .and_then(|r| r.as_str())
                    .unwrap_or("")
                    .to_string();
                let groups = entry
                    .get("groups")
                    .and_then(|g| g.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|g| g.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                repos.push(Repo {
                    name,
                    path,
                    rev,
                    groups,
                });
            }
        }

        Ok(Context {
            root,
            stack,
            phase,
            repos,
        })
    }

    /// Best-effort changeset id from a `change/<ID>` branch in any repo `rev`.
    pub fn changeset_from_revs(&self) -> Option<String> {
        self.repos
            .iter()
            .find_map(|r| changeset_from_branch(&r.rev))
    }
}

/// Extract `<ID>` from a `change/<ID>` branch name.
pub fn changeset_from_branch(branch: &str) -> Option<String> {
    branch
        .strip_prefix("change/")
        .filter(|rest| !rest.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_context() {
        let raw = r#"{
            "schema": "haw.plugin/1",
            "root": "/ws",
            "stack": "gateway",
            "phase": "post-land",
            "repos": [
                { "name": "api", "path": "/ws/api", "rev": "change/PROJ-42", "groups": ["core"] }
            ]
        }"#;
        let ctx = Context::from_json(raw).unwrap();
        assert_eq!(ctx.stack.as_deref(), Some("gateway"));
        assert_eq!(ctx.phase.as_deref(), Some("post-land"));
        assert_eq!(ctx.changeset_from_revs().as_deref(), Some("PROJ-42"));
    }

    #[test]
    fn parses_degraded_context() {
        let ctx = Context::from_json(r#"{ "schema": "haw.plugin/1" }"#).unwrap();
        assert!(ctx.repos.is_empty());
        assert_eq!(ctx.changeset_from_revs(), None);
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(Context::from_json("").unwrap(), Context::default());
    }

    #[test]
    fn rejects_wrong_schema() {
        assert!(Context::from_json(r#"{ "schema": "x/1" }"#).is_err());
    }

    #[test]
    fn branch_parsing() {
        assert_eq!(
            changeset_from_branch("change/PROJ-1").as_deref(),
            Some("PROJ-1")
        );
        assert_eq!(changeset_from_branch("main"), None);
        assert_eq!(changeset_from_branch("change/"), None);
    }
}
