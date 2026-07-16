//! Reads and parses the `haw.plugin/1` input context.
//!
//! Parsing is deliberately defensive: run outside a workspace, haw degrades the
//! context to `{ "schema": "haw.plugin/1" }`, so every field beyond `schema` is
//! optional and must never cause a panic when absent.

use std::path::PathBuf;

use haw_core::plugin::CONTRACT;

/// A single repository entry from the `haw.plugin/1` context.
#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    /// The repository's declared name.
    pub name: String,
    /// The absolute path to the repository working tree.
    pub path: PathBuf,
    /// The pinned revision (tag, branch, or SHA) for this repo.
    pub rev: String,
    /// The groups this repo belongs to.
    pub groups: Vec<String>,
}

/// The parsed `haw.plugin/1` context relevant to this plugin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context {
    /// The absolute workspace root, if inside a workspace.
    pub root: Option<PathBuf>,
    /// The selected stack name, if any.
    pub stack: Option<String>,
    /// The lifecycle phase, if invoked as a hook.
    pub phase: Option<String>,
    /// The repositories declared in the context.
    pub repos: Vec<Repo>,
}

/// Read the raw context JSON from `HAW_JSON`, falling back to stdin.
///
/// Returns an empty string when neither source carries a context (e.g. a plain
/// `haw-aspice --help` invocation with nothing piped in); callers should treat
/// that as the degraded, workspace-less context.
pub fn read_raw_context() -> String {
    if let Ok(value) = std::env::var("HAW_JSON")
        && !value.trim().is_empty()
    {
        return value;
    }
    use std::io::Read;
    let mut buf = String::new();
    // A missing/closed stdin simply yields the degraded context; never panic.
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        return String::new();
    }
    buf
}

impl Context {
    /// Parse a `haw.plugin/1` context from a raw JSON string.
    ///
    /// An empty string parses to the degraded, workspace-less context. A
    /// non-empty document must declare `schema == "haw.plugin/1"`.
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
                { "name": "kernel", "path": "/ws/kernel", "rev": "v6.1.2", "groups": ["firmware"] }
            ]
        }"#;
        let ctx = Context::from_json(raw).unwrap();
        assert_eq!(ctx.root, Some(PathBuf::from("/ws")));
        assert_eq!(ctx.stack.as_deref(), Some("gateway"));
        assert_eq!(ctx.phase.as_deref(), Some("post-land"));
        assert_eq!(ctx.repos.len(), 1);
        assert_eq!(ctx.repos[0].name, "kernel");
        assert_eq!(ctx.repos[0].rev, "v6.1.2");
        assert_eq!(ctx.repos[0].groups, vec!["firmware".to_string()]);
    }

    #[test]
    fn parses_degraded_schema_only_context() {
        let ctx = Context::from_json(r#"{ "schema": "haw.plugin/1" }"#).unwrap();
        assert_eq!(ctx.root, None);
        assert_eq!(ctx.stack, None);
        assert!(ctx.repos.is_empty());
    }

    #[test]
    fn empty_string_is_the_degraded_context() {
        let ctx = Context::from_json("").unwrap();
        assert_eq!(ctx, Context::default());
    }

    #[test]
    fn rejects_wrong_schema() {
        assert!(Context::from_json(r#"{ "schema": "nope/9" }"#).is_err());
    }

    #[test]
    fn tolerates_missing_repo_fields() {
        let raw = r#"{ "schema": "haw.plugin/1", "repos": [ { "path": "/ws/a" } ] }"#;
        let ctx = Context::from_json(raw).unwrap();
        assert_eq!(ctx.repos.len(), 1);
        assert_eq!(ctx.repos[0].name, "<unnamed>");
        assert_eq!(ctx.repos[0].rev, "");
        assert!(ctx.repos[0].groups.is_empty());
    }
}
