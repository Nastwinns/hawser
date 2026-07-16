//! Building the composition-level SBOM documents (CycloneDX 1.5 and SPDX 2.3).
//!
//! Each repository in the context becomes a single component: its name, a
//! version equal to the pinned revision (SHA), a purl-like identifier
//! (`pkg:generic/<name>@<rev>`) and a supplier. Both documents carry the
//! NTIA-minimum fields: supplier, component name, version, unique id, dependency
//! relationship, author and timestamp.
//!
//! Determinism: components are emitted in the already-sorted order of the
//! context, and the timestamp comes from `SOURCE_DATE_EPOCH` (or is omitted) —
//! never wall-clock time — so two runs with the same input produce byte-identical
//! output.

use serde_json::{Value, json};

use crate::context::{Context, Repo};

/// The supplier/author name embedded in generated SBOMs.
const AUTHOR: &str = "haw-compliance";

/// External scanners that, if present on `PATH`, could enrich the SBOM.
const SCANNERS: &[&str] = &["syft", "cargo-cyclonedx"];

/// The version string used for a repo, falling back when no `rev` is pinned.
fn repo_version(repo: &Repo) -> &str {
    repo.rev.as_deref().unwrap_or("unknown")
}

/// The purl-like identifier for a repo component.
fn repo_purl(repo: &Repo) -> String {
    format!("pkg:generic/{}@{}", repo.name, repo_version(repo))
}

/// A stable BOM-reference / SPDXID-friendly slug for a repo.
fn repo_slug(repo: &Repo) -> String {
    let sanitized: String = repo
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{sanitized}-{}", repo_version(repo))
}

/// Compute a deterministic timestamp string from `SOURCE_DATE_EPOCH`.
///
/// Returns an RFC 3339 UTC timestamp when `SOURCE_DATE_EPOCH` is set to a valid
/// non-negative integer, or `None` (meaning: omit the timestamp) otherwise. Wall
/// clock time is never consulted, keeping output deterministic.
pub fn deterministic_timestamp() -> Option<String> {
    let raw = std::env::var("SOURCE_DATE_EPOCH").ok()?;
    let secs: i64 = raw.trim().parse().ok()?;
    if secs < 0 {
        return None;
    }
    Some(format_epoch_utc(secs))
}

/// Format a non-negative Unix timestamp (seconds) as an RFC 3339 UTC string.
///
/// Implemented from first principles (civil-from-days algorithm) to avoid any
/// non-workspace date dependency.
fn format_epoch_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Detect whether a per-ecosystem scanner is available on `PATH`.
///
/// Returns the first matching scanner name, or `None` if none are present.
pub fn detect_scanner() -> Option<String> {
    for scanner in SCANNERS {
        if which_on_path(scanner) {
            return Some((*scanner).to_string());
        }
    }
    None
}

/// Return true if `program` is found on `PATH` (cross-platform).
fn which_on_path(program: &str) -> bool {
    let path = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.BAT;.CMD".to_string())
            .split(';')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

/// Build the CycloneDX 1.5 JSON document for the given context.
pub fn cyclonedx_document(ctx: &Context, timestamp: Option<&str>) -> Value {
    let components: Vec<Value> = ctx
        .repos
        .iter()
        .map(|repo| {
            json!({
                "type": "application",
                "bom-ref": repo_slug(repo),
                "name": repo.name,
                "version": repo_version(repo),
                "purl": repo_purl(repo),
                "supplier": { "name": AUTHOR }
            })
        })
        .collect();

    let dependencies: Vec<Value> = ctx
        .repos
        .iter()
        .map(|repo| json!({ "ref": repo_slug(repo), "dependsOn": [] }))
        .collect();

    let mut metadata = json!({
        "authors": [ { "name": AUTHOR } ],
        "component": {
            "type": "application",
            "bom-ref": "haw-composition",
            "name": "haw-composition"
        }
    });
    if let Some(ts) = timestamp
        && let Some(obj) = metadata.as_object_mut()
    {
        obj.insert("timestamp".to_string(), Value::String(ts.to_string()));
    }

    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": metadata,
        "components": components,
        "dependencies": dependencies
    })
}

/// Build the SPDX 2.3 JSON document for the given context.
pub fn spdx_document(ctx: &Context, timestamp: Option<&str>) -> Value {
    let root_spdx_id = "SPDXRef-DOCUMENT";

    let packages: Vec<Value> = ctx
        .repos
        .iter()
        .map(|repo| {
            json!({
                "SPDXID": format!("SPDXRef-Package-{}", repo_slug(repo)),
                "name": repo.name,
                "versionInfo": repo_version(repo),
                "supplier": format!("Organization: {AUTHOR}"),
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": false,
                "externalRefs": [
                    {
                        "referenceCategory": "PACKAGE-MANAGER",
                        "referenceType": "purl",
                        "referenceLocator": repo_purl(repo)
                    }
                ]
            })
        })
        .collect();

    let mut relationships: Vec<Value> = ctx
        .repos
        .iter()
        .map(|repo| {
            json!({
                "spdxElementId": root_spdx_id,
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": format!("SPDXRef-Package-{}", repo_slug(repo))
            })
        })
        .collect();

    // Emit a DEPENDS_ON chain so the dependency relationship field is present.
    for pair in ctx.repos.windows(2) {
        if let [from, to] = pair {
            relationships.push(json!({
                "spdxElementId": format!("SPDXRef-Package-{}", repo_slug(from)),
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": format!("SPDXRef-Package-{}", repo_slug(to))
            }));
        }
    }

    let created = timestamp.unwrap_or("1970-01-01T00:00:00Z");

    json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": root_spdx_id,
        "name": "haw-composition",
        "documentNamespace": "https://haw.invalid/sbom/haw-composition",
        "creationInfo": {
            "created": created,
            "creators": [ format!("Tool: {AUTHOR}") ]
        },
        "packages": packages,
        "relationships": relationships
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    fn fixture() -> Context {
        Context::from_json(
            r#"{
                "schema": "haw.plugin/1",
                "root": "/ws",
                "repos": [
                    {"name": "kernel", "path": "/ws/k", "rev": "abc123", "groups": []},
                    {"name": "shell", "path": "/ws/s", "rev": "def456", "groups": []}
                ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn cyclonedx_contains_both_components_with_shas() {
        let cdx = cyclonedx_document(&fixture(), Some("2021-01-01T00:00:00Z"));
        let text = serde_json::to_string(&cdx).unwrap();
        assert!(text.contains("kernel"));
        assert!(text.contains("shell"));
        assert!(text.contains("abc123"));
        assert!(text.contains("def456"));
        assert!(text.contains("pkg:generic/kernel@abc123"));
        // Valid JSON round-trip.
        let _: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(cdx["specVersion"], "1.5");
        assert_eq!(cdx["components"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn spdx_contains_both_components_with_shas() {
        let spdx = spdx_document(&fixture(), Some("2021-01-01T00:00:00Z"));
        let text = serde_json::to_string(&spdx).unwrap();
        assert!(text.contains("kernel"));
        assert!(text.contains("shell"));
        assert!(text.contains("abc123"));
        assert!(text.contains("def456"));
        let _: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(spdx["spdxVersion"], "SPDX-2.3");
        assert_eq!(spdx["packages"].as_array().unwrap().len(), 2);
        // NTIA minimum: supplier present on each package.
        for pkg in spdx["packages"].as_array().unwrap() {
            assert!(pkg["supplier"].as_str().unwrap().contains("haw-compliance"));
        }
    }

    #[test]
    fn deterministic_across_runs() {
        let ctx = fixture();
        let a_cdx =
            serde_json::to_string_pretty(&cyclonedx_document(&ctx, Some("1609459200"))).unwrap();
        let b_cdx =
            serde_json::to_string_pretty(&cyclonedx_document(&ctx, Some("1609459200"))).unwrap();
        assert_eq!(a_cdx, b_cdx);

        let a_spdx =
            serde_json::to_string_pretty(&spdx_document(&ctx, Some("1609459200"))).unwrap();
        let b_spdx =
            serde_json::to_string_pretty(&spdx_document(&ctx, Some("1609459200"))).unwrap();
        assert_eq!(a_spdx, b_spdx);
    }

    #[test]
    fn timestamp_from_epoch_is_correct() {
        // 1609459200 == 2021-01-01T00:00:00Z
        assert_eq!(format_epoch_utc(1_609_459_200), "2021-01-01T00:00:00Z");
        // 0 == unix epoch
        assert_eq!(format_epoch_utc(0), "1970-01-01T00:00:00Z");
        // A known point with time-of-day.
        assert_eq!(format_epoch_utc(1_700_000_000), "2023-11-14T22:13:20Z");
    }
}
