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

/// A repo discovered from `haw status --format json` enrichment.
#[derive(Debug, Clone, Default)]
pub struct EnrichedRepo {
    /// The observed pinned SHA (`sha` / `commit` / `rev.commit`), if any.
    pub sha: Option<String>,
    /// The groups reported by `haw status`, if any.
    pub groups: Vec<String>,
}

/// Enrichment gathered from `haw status --format json`, when available.
#[derive(Debug, Default)]
pub struct Enrichment {
    /// Map of repo name -> observed pinned SHA (`rev.commit` / `sha`).
    pub shas: std::collections::HashMap<String, String>,
    /// Map of repo name -> enriched repo detail (sha + groups), in the order
    /// `haw status` reported them.
    pub repos: Vec<(String, EnrichedRepo)>,
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
    let mut enriched = Vec::new();
    if let Some(repos) = value.get("repos").and_then(|r| r.as_array()) {
        for repo in repos {
            let name = match repo.get("name").and_then(|n| n.as_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };
            let sha = repo
                .get("sha")
                .and_then(|s| s.as_str())
                .or_else(|| repo.get("commit").and_then(|c| c.as_str()))
                .or_else(|| {
                    repo.get("rev")
                        .and_then(|r| r.get("commit"))
                        .and_then(|c| c.as_str())
                })
                .map(str::to_string);
            let groups = repo
                .get("groups")
                .and_then(|g| g.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|g| g.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(sha) = &sha {
                shas.insert(name.clone(), sha.clone());
            }
            enriched.push((name, EnrichedRepo { sha, groups }));
        }
    }
    Enrichment {
        shas,
        repos: enriched,
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

/// A repo to trace, resolved from a merge of context and enrichment.
///
/// The context (`haw.plugin/1`) is authoritative when populated; when the
/// plain-subcommand dispatch leaves `ctx.repos` empty we fall back to the repos
/// that `haw status --format json` discovered so the artifacts never disagree
/// with the summary count.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveRepo {
    /// Repo name.
    pub name: String,
    /// Absolute path, when known (context only).
    pub path: Option<PathBuf>,
    /// The context rev, when known.
    pub rev: Option<String>,
    /// The resolved pinned SHA.
    pub pinned_sha: String,
    /// Groups, from context or enrichment.
    pub groups: Vec<String>,
}

/// Build the effective repo list: prefer `ctx.repos`, else fall back to the
/// repos discovered by `enrich_from_haw`.
pub fn effective_repos(ctx: &Context, enrich: &Enrichment) -> Vec<EffectiveRepo> {
    if !ctx.repos.is_empty() {
        return ctx
            .repos
            .iter()
            .map(|r| EffectiveRepo {
                name: r.name.clone(),
                path: Some(r.path.clone()),
                rev: Some(r.rev.clone()),
                pinned_sha: pinned_sha(r, enrich),
                groups: r.groups.clone(),
            })
            .collect();
    }
    enrich
        .repos
        .iter()
        .map(|(name, detail)| EffectiveRepo {
            name: name.clone(),
            path: None,
            rev: None,
            pinned_sha: detail.sha.clone().unwrap_or_default(),
            groups: detail.groups.clone(),
        })
        .collect()
}

/// Build the machine `haw.aspice/1` document.
pub fn build_json(ctx: &Context, enrich: &Enrichment, timestamp: Option<&str>) -> Value {
    let repos: Vec<Value> = effective_repos(ctx, enrich)
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "path": r.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                "rev": r.rev,
                "pinned_sha": r.pinned_sha,
                "groups": r.groups,
                "process_areas": SWE_PROCESS_AREAS,
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
///
/// Each repo is traced against the software-engineering process group.
pub const SWE_PROCESS_AREAS: [&str; 6] = ["SWE.1", "SWE.2", "SWE.3", "SWE.4", "SWE.5", "SWE.6"];

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
    let repos = effective_repos(ctx, enrich);
    if repos.is_empty() {
        out.push_str("_No repositories._\n");
        return out;
    }

    out.push_str("| Repo | Pinned SHA | Groups | Process Areas |\n");
    out.push_str("|------|-----------|--------|---------------|\n");
    for repo in &repos {
        let groups = if repo.groups.is_empty() {
            "-".to_string()
        } else {
            repo.groups.join(", ")
        };
        out.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            repo.name,
            repo.pinned_sha,
            groups,
            SWE_PROCESS_AREAS.join(", "),
        ));
    }
    out
}

/// A one-line summary for the hook report / human output.
///
/// Counts the same effective repo list the artifacts iterate, so the summary
/// never disagrees with the SWE table / JSON `repos` array.
pub fn summary(ctx: &Context, enrich: &Enrichment) -> String {
    format!(
        "aspice: traced {} repos @ pinned SHAs",
        effective_repos(ctx, enrich).len()
    )
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
        assert!(md.contains("_No repositories._"));
    }

    /// Enrichment carrying two repos (simulating `haw status --format json`),
    /// with no network access.
    fn enrichment_with_two() -> Enrichment {
        let mut enrich = Enrichment {
            from_haw: true,
            ..Enrichment::default()
        };
        for (name, sha) in [("alpha", "aaa111"), ("beta", "bbb222")] {
            enrich.shas.insert(name.to_string(), sha.to_string());
            enrich.repos.push((
                name.to_string(),
                EnrichedRepo {
                    sha: Some(sha.to_string()),
                    groups: vec!["svc".to_string()],
                },
            ));
        }
        enrich
    }

    #[test]
    fn effective_repos_fall_back_to_enrichment_when_ctx_empty() {
        let empty = Context::default();
        let enrich = enrichment_with_two();

        // build_markdown emits 2 table rows.
        let md = build_markdown(&empty, &enrich, None);
        let rows = md
            .lines()
            .filter(|l| l.starts_with("| ") && !l.contains("Repo |") && !l.contains("---"))
            .count();
        assert_eq!(rows, 2, "expected 2 SWE table rows, got:\n{md}");
        assert!(md.contains("| alpha |"));
        assert!(md.contains("`bbb222`"));

        // build_json's repos array has length 2.
        let doc = build_json(&empty, &enrich, None);
        let repos = doc["repos"].as_array().unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0]["name"], "alpha");
        assert_eq!(repos[0]["pinned_sha"], "aaa111");
        assert_eq!(repos[1]["groups"][0], "svc");

        // Summary counts the same effective list.
        assert_eq!(
            summary(&empty, &enrich),
            "aspice: traced 2 repos @ pinned SHAs"
        );
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
        assert_eq!(
            summary(&ctx(), &Enrichment::default()),
            "aspice: traced 1 repos @ pinned SHAs"
        );
    }
}
