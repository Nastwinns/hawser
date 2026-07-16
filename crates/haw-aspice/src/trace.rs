//! Builds the ASPICE traceability bundle from the plugin context.
//!
//! Two artifacts are produced:
//! - `aspice-trace.json` — machine document, schema `haw.aspice/1`.
//! - `aspice-trace.md` — human report mapping repos to ASPICE process areas.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::context::Context;

/// The machine-document schema this plugin emits.
pub const ASPICE_SCHEMA: &str = "haw.aspice/1";

/// The filenames written into the output directory.
pub const JSON_ARTIFACT: &str = "aspice-trace.json";
pub const MD_ARTIFACT: &str = "aspice-trace.md";

/// Resolve the timestamp to stamp into the trace.
///
/// Preference order: explicit `--at`, then `SOURCE_DATE_EPOCH` (rendered as a
/// bare epoch-seconds marker since no date library is available), else `None`
/// (omit the field entirely — reproducible by default).
pub fn resolve_timestamp(at: Option<&str>) -> Option<String> {
    if let Some(explicit) = at {
        return Some(explicit.to_string());
    }
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH")
        && !epoch.trim().is_empty()
        && epoch.trim().chars().all(|c| c.is_ascii_digit())
    {
        return Some(format!("epoch:{}", epoch.trim()));
    }
    None
}

/// Choose the output directory: `--out-dir`, else the workspace root, else cwd.
pub fn resolve_out_dir(out_dir: Option<&str>, ctx: &Context) -> PathBuf {
    if let Some(dir) = out_dir {
        return PathBuf::from(dir);
    }
    if let Some(root) = &ctx.root {
        return root.clone();
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Enrichment gathered from `haw status --format json`, when available.
#[derive(Debug, Default)]
pub struct Enrichment {
    /// Map of repo name -> observed pinned SHA (`rev.commit` / `sha`).
    pub shas: std::collections::HashMap<String, String>,
    /// Whether the `haw` CLI was found and produced parseable JSON.
    pub from_haw: bool,
}

/// Shell out to `haw status --format json` to enrich pinned SHAs.
///
/// Tolerates `haw` being absent from PATH and any non-JSON output — enrichment
/// is best-effort and never fails the run.
pub fn enrich_from_haw(root: Option<&Path>) -> Enrichment {
    let mut cmd = std::process::Command::new("haw");
    cmd.arg("status").arg("--format").arg("json");
    if let Some(root) = root {
        cmd.current_dir(root);
    }
    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return Enrichment::default(),
    };
    if !output.status.success() {
        return Enrichment::default();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value: Value = match serde_json::from_str(text.trim()) {
        Ok(v) => v,
        Err(_) => return Enrichment::default(),
    };

    let mut shas = std::collections::HashMap::new();
    if let Some(repos) = value.get("repos").and_then(|r| r.as_array()) {
        for repo in repos {
            let name = repo.get("name").and_then(|n| n.as_str());
            let sha = repo
                .get("sha")
                .and_then(|s| s.as_str())
                .or_else(|| repo.get("commit").and_then(|c| c.as_str()))
                .or_else(|| {
                    repo.get("rev")
                        .and_then(|r| r.get("commit"))
                        .and_then(|c| c.as_str())
                });
            if let (Some(name), Some(sha)) = (name, sha) {
                shas.insert(name.to_string(), sha.to_string());
            }
        }
    }
    Enrichment {
        shas,
        from_haw: true,
    }
}

/// The pinned SHA for a repo: prefer haw's observed SHA, else the context rev.
fn pinned_sha(repo: &crate::context::Repo, enrich: &Enrichment) -> String {
    enrich
        .shas
        .get(&repo.name)
        .cloned()
        .unwrap_or_else(|| repo.rev.clone())
}

/// Build the machine `haw.aspice/1` document.
pub fn build_json(ctx: &Context, enrich: &Enrichment, timestamp: Option<&str>) -> Value {
    let repos: Vec<Value> = ctx
        .repos
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "path": r.path.to_string_lossy(),
                "rev": r.rev,
                "pinned_sha": pinned_sha(r, enrich),
                "groups": r.groups,
                "process_areas": process_areas_for(r),
            })
        })
        .collect();

    let mut doc = json!({
        "schema": ASPICE_SCHEMA,
        "root": ctx.root.as_ref().map(|p| p.to_string_lossy().into_owned()),
        "stack": ctx.stack,
        "config_management": {
            "process_areas": ["MAN.3", "SUP.8"],
            "mechanism": "haw lockfile (pinned SHAs)",
            "source": if enrich.from_haw { "haw status --format json" } else { "haw.plugin/1 context" },
        },
        "change_request": {
            "process_area": "SUP.10",
            "stack": ctx.stack,
        },
        "repos": repos,
    });

    if let Some(ts) = timestamp
        && let Some(map) = doc.as_object_mut()
    {
        map.insert("generated_at".to_string(), json!(ts));
    }
    doc
}

/// The ASPICE software-engineering process areas traced per repo.
fn process_areas_for(_repo: &crate::context::Repo) -> Vec<&'static str> {
    // Each repo is traced against the software-engineering process group.
    vec!["SWE.1", "SWE.2", "SWE.3", "SWE.4", "SWE.5", "SWE.6"]
}

/// Build the human-readable markdown report.
pub fn build_markdown(ctx: &Context, enrich: &Enrichment, timestamp: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("# ASPICE Traceability Report\n\n");

    match &ctx.root {
        Some(root) => out.push_str(&format!("- **Workspace root:** `{}`\n", root.display())),
        None => out.push_str("- **Workspace root:** _(none — run outside a workspace)_\n"),
    }
    out.push_str(&format!(
        "- **Stack:** {}\n",
        ctx.stack.as_deref().unwrap_or("_(none)_")
    ));
    if let Some(ts) = timestamp {
        out.push_str(&format!("- **Generated at:** {ts}\n"));
    }
    out.push_str(&format!(
        "- **Enrichment:** {}\n",
        if enrich.from_haw {
            "haw status --format json"
        } else {
            "context only (haw not on PATH)"
        }
    ));
    out.push('\n');

    out.push_str("## Configuration Management (MAN.3, SUP.8)\n\n");
    out.push_str(
        "Configuration is managed through the haw lockfile: every repo below is \
pinned to a specific SHA, giving a reproducible, auditable fleet state.\n\n",
    );

    out.push_str("## Change Request (SUP.10)\n\n");
    out.push_str(&format!(
        "Current stack `{}` frames the active changeset for this traceability \
snapshot.\n\n",
        ctx.stack.as_deref().unwrap_or("(none)")
    ));

    out.push_str("## Software Engineering per Repository (SWE.1–SWE.6)\n\n");
    if ctx.repos.is_empty() {
        out.push_str("_No repositories in context._\n");
        return out;
    }

    out.push_str("| Repo | Pinned SHA | Groups | Process Areas |\n");
    out.push_str("|------|-----------|--------|---------------|\n");
    for repo in &ctx.repos {
        let sha = pinned_sha(repo, enrich);
        let groups = if repo.groups.is_empty() {
            "-".to_string()
        } else {
            repo.groups.join(", ")
        };
        out.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            repo.name,
            sha,
            groups,
            process_areas_for(repo).join(", "),
        ));
    }
    out
}

/// A one-line summary for the hook report / human output.
pub fn summary(ctx: &Context) -> String {
    format!("aspice: traced {} repos @ pinned SHAs", ctx.repos.len())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::context::Repo;

    fn ctx() -> Context {
        Context {
            root: Some(PathBuf::from("/ws")),
            stack: Some("gateway".to_string()),
            phase: None,
            repos: vec![Repo {
                name: "kernel".to_string(),
                path: PathBuf::from("/ws/kernel"),
                rev: "v6.1.2".to_string(),
                groups: vec!["firmware".to_string()],
            }],
        }
    }

    #[test]
    fn json_uses_context_rev_without_enrichment() {
        let doc = build_json(&ctx(), &Enrichment::default(), None);
        assert_eq!(doc["schema"], ASPICE_SCHEMA);
        assert_eq!(doc["repos"][0]["pinned_sha"], "v6.1.2");
        assert!(doc.get("generated_at").is_none());
    }

    #[test]
    fn json_prefers_enriched_sha() {
        let mut enrich = Enrichment::default();
        enrich
            .shas
            .insert("kernel".to_string(), "abc123".to_string());
        let doc = build_json(&ctx(), &enrich, Some("2026-07-16"));
        assert_eq!(doc["repos"][0]["pinned_sha"], "abc123");
        assert_eq!(doc["generated_at"], "2026-07-16");
    }

    #[test]
    fn markdown_contains_repo_table_row() {
        let md = build_markdown(&ctx(), &Enrichment::default(), None);
        assert!(md.contains("| kernel |"));
        assert!(md.contains("MAN.3"));
        assert!(md.contains("SWE.1"));
    }

    #[test]
    fn markdown_handles_empty_fleet() {
        let empty = Context::default();
        let md = build_markdown(&empty, &Enrichment::default(), None);
        assert!(md.contains("No repositories in context"));
    }

    #[test]
    fn resolve_timestamp_prefers_explicit() {
        assert_eq!(
            resolve_timestamp(Some("2026-01-01")).as_deref(),
            Some("2026-01-01")
        );
    }

    #[test]
    fn resolve_out_dir_prefers_flag_then_root() {
        let c = ctx();
        assert_eq!(resolve_out_dir(Some("/tmp/x"), &c), PathBuf::from("/tmp/x"));
        assert_eq!(resolve_out_dir(None, &c), PathBuf::from("/ws"));
    }

    #[test]
    fn summary_counts_repos() {
        assert_eq!(summary(&ctx()), "aspice: traced 1 repos @ pinned SHAs");
    }
}
