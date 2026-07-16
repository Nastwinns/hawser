//! Reads and parses the `haw.plugin/1` input context using raw `serde_json`.

use std::path::PathBuf;

/// A single repository entry from the `haw.plugin/1` context.
pub struct Repo {
    /// The repository's declared name.
    pub name: String,
    /// The absolute path to the repository working tree.
    pub path: PathBuf,
}

/// The parsed `haw.plugin/1` context relevant to this plugin.
pub struct Context {
    /// The repositories declared in the context.
    pub repos: Vec<Repo>,
}

/// Read the raw context JSON from `HAW_JSON`, falling back to stdin.
pub fn read_raw_context() -> Result<String, String> {
    if let Ok(value) = std::env::var("HAW_JSON")
        && !value.trim().is_empty()
    {
        return Ok(value);
    }
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("failed to read context from stdin: {e}"))?;
    if buf.trim().is_empty() {
        return Err("no context provided on HAW_JSON or stdin".to_string());
    }
    Ok(buf)
}

impl Context {
    /// Parse a `haw.plugin/1` context from a raw JSON string.
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| format!("invalid context JSON: {e}"))?;

        let schema = value.get("schema").and_then(|s| s.as_str());
        if schema != Some("haw.plugin/1") {
            return Err(format!(
                "unexpected context schema: {}",
                schema.unwrap_or("<missing>")
            ));
        }

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
                repos.push(Repo { name, path });
            }
        }

        Ok(Context { repos })
    }
}
