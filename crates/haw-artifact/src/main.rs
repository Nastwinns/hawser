//! `haw-artifact` — a standalone haw plugin binary that produces build provenance
//! and orchestrates signing of that provenance.
//!
//! It consumes the `haw.plugin/1` context (from the `HAW_JSON` environment variable,
//! falling back to stdin), emits an in-toto / SLSA-style provenance statement binding
//! subject artifacts to the pinned repository materials, optionally signs the statement
//! by shelling out to `cosign` or `minisign`, and can verify a previously produced
//! statement against the current context and on-disk artifacts.
//!
//! All output is deterministic: digests are derived from file bytes and any timestamp
//! is taken from `SOURCE_DATE_EPOCH` (never the wall clock).

use std::collections::BTreeMap;
use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Schema identifier for the input plugin context.
const INPUT_SCHEMA: &str = "haw.plugin/1";
/// Schema identifier for the emitted plugin report.
const REPORT_SCHEMA: &str = "haw.plugin.report/1";
/// Plugin name reported in the output.
const PLUGIN_NAME: &str = "haw-artifact";
/// in-toto Statement type URI.
const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
/// SLSA provenance predicate type URI.
const PREDICATE_TYPE: &str = "https://slsa.dev/provenance/v1";
/// Default output directory for produced artifacts.
const DEFAULT_OUT: &str = ".haw/provenance";

/// A single repository material drawn from the plugin context.
#[derive(Debug, Clone, Deserialize)]
struct Repo {
    name: String,
    path: String,
    rev: String,
    #[serde(default)]
    groups: Vec<String>,
}

/// The decoded `haw.plugin/1` context.
#[derive(Debug, Clone, Deserialize)]
struct Context {
    root: String,
    #[serde(default)]
    stack: Option<String>,
    #[serde(default)]
    repos: Vec<Repo>,
}

/// A finding surfaced in the report (severity is one of `info`, `warn`, `error`).
#[derive(Debug, Clone, Serialize)]
struct Finding {
    severity: String,
    message: String,
}

impl Finding {
    /// Builds a `warn`-level finding.
    fn warn(message: impl Into<String>) -> Self {
        Finding {
            severity: "warn".into(),
            message: message.into(),
        }
    }

    /// Builds an `error`-level finding.
    fn error(message: impl Into<String>) -> Self {
        Finding {
            severity: "error".into(),
            message: message.into(),
        }
    }

    /// Builds an `info`-level finding.
    fn info(message: impl Into<String>) -> Self {
        Finding {
            severity: "info".into(),
            message: message.into(),
        }
    }
}

/// An artifact produced by the plugin (`kind` is `provenance` or `signature`).
#[derive(Debug, Clone, Serialize)]
struct Artifact {
    path: String,
    kind: String,
}

/// The accumulated result of a run, serialized as `haw.plugin.report/1`.
#[derive(Debug, Default)]
struct Report {
    ok: bool,
    summary: String,
    artifacts: Vec<Artifact>,
    findings: Vec<Finding>,
}

impl Report {
    /// Renders the report as a `haw.plugin.report/1` JSON value.
    fn to_json(&self, phase: Option<&str>) -> Value {
        json!({
            "schema": REPORT_SCHEMA,
            "plugin": PLUGIN_NAME,
            "phase": phase,
            "ok": self.ok,
            "summary": self.summary,
            "artifacts": self.artifacts,
            "findings": self.findings,
        })
    }
}

/// Parsed command-line arguments.
#[derive(Debug, Default)]
struct Cli {
    phase: Option<String>,
    out: Option<PathBuf>,
    subjects: Vec<PathBuf>,
    sign: bool,
    verify: Option<PathBuf>,
    help: bool,
}

/// Error type for all fallible plugin operations.
#[derive(Debug)]
enum ArtifactError {
    Usage(String),
    Io(String),
    Parse(String),
    Digest(String),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactError::Usage(m) => write!(f, "usage error: {m}"),
            ArtifactError::Io(m) => write!(f, "io error: {m}"),
            ArtifactError::Parse(m) => write!(f, "parse error: {m}"),
            ArtifactError::Digest(m) => write!(f, "digest error: {m}"),
        }
    }
}

impl std::error::Error for ArtifactError {}

/// Usage text shown for `--help`.
const HELP: &str = "\
haw-artifact — build provenance and signature orchestration

USAGE:
    haw-artifact [OPTIONS]

OPTIONS:
    --haw-phase <name>     Phase name echoed into the report
    --out <dir>            Output directory (default: .haw/provenance)
    --subject <file>       Artifact to attest/sign (repeatable)
    --sign                 Attempt signing if cosign/minisign is on PATH
    --verify <file>        Verify an existing provenance statement (fail-closed)
    --help                 Print this help

INPUT:
    haw.plugin/1 context via HAW_JSON env var (fallback: stdin)

OUTPUT:
    haw.plugin.report/1 JSON on stdout
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(err) => {
            let report = Report {
                ok: false,
                summary: format!("{err}"),
                artifacts: Vec::new(),
                findings: vec![Finding::error(format!("{err}"))],
            };
            print_report(&report, None);
            ExitCode::FAILURE
        }
    }
}

/// Drives argument parsing, dispatch and reporting; returns the process exit code.
fn run(args: &[String]) -> Result<ExitCode, ArtifactError> {
    let cli = parse_args(args)?;
    if cli.help {
        print!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    }

    let phase = cli.phase.clone();

    if let Some(prov_path) = cli.verify.clone() {
        let ctx = read_context()?;
        let report = verify(&ctx, &prov_path)?;
        print_report(&report, phase.as_deref());
        return Ok(if report.ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }

    let ctx = read_context()?;
    let report = generate(&ctx, &cli)?;
    print_report(&report, phase.as_deref());
    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Parses the supported command-line flags.
fn parse_args(args: &[String]) -> Result<Cli, ArtifactError> {
    let mut cli = Cli::default();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--help" | "-h" => cli.help = true,
            "--sign" => cli.sign = true,
            "--haw-phase" => {
                cli.phase = Some(take_value(args, &mut i, "--haw-phase")?);
            }
            "--out" => {
                cli.out = Some(PathBuf::from(take_value(args, &mut i, "--out")?));
            }
            "--subject" => {
                cli.subjects
                    .push(PathBuf::from(take_value(args, &mut i, "--subject")?));
            }
            "--verify" => {
                cli.verify = Some(PathBuf::from(take_value(args, &mut i, "--verify")?));
            }
            other => {
                return Err(ArtifactError::Usage(format!("unknown argument: {other}")));
            }
        }
        i += 1;
    }
    Ok(cli)
}

/// Consumes the value that follows a flag, advancing the index past it.
fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, ArtifactError> {
    let Some(v) = args.get(*i + 1) else {
        return Err(ArtifactError::Usage(format!("{flag} requires a value")));
    };
    *i += 1;
    Ok(v.clone())
}

/// Reads and decodes the `haw.plugin/1` context from `HAW_JSON` or stdin.
fn read_context() -> Result<Context, ArtifactError> {
    let raw = match env::var("HAW_JSON") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| ArtifactError::Io(format!("reading stdin: {e}")))?;
            buf
        }
    };
    parse_context(&raw)
}

/// Parses a `haw.plugin/1` context from a raw JSON string.
fn parse_context(raw: &str) -> Result<Context, ArtifactError> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|e| ArtifactError::Parse(format!("invalid context JSON: {e}")))?;
    let schema = value.get("schema").and_then(Value::as_str).unwrap_or("");
    if schema != INPUT_SCHEMA {
        return Err(ArtifactError::Parse(format!(
            "unexpected context schema: expected {INPUT_SCHEMA}, got {schema}"
        )));
    }
    serde_json::from_value(value)
        .map_err(|e| ArtifactError::Parse(format!("malformed context: {e}")))
}

/// Reads the optional `SOURCE_DATE_EPOCH` value, if it parses as an integer.
fn source_date_epoch() -> Option<i64> {
    env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
}

/// Reads the builder identity from `HAW_BUILDER`, defaulting to `haw`.
fn builder_id() -> String {
    match env::var("HAW_BUILDER") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => "haw".to_string(),
    }
}

/// Computes the SHA-256 digest of a file, shelling out to the platform tool.
fn sha256_file(path: &Path) -> Result<String, ArtifactError> {
    let display = path.display().to_string();
    if !path.exists() {
        return Err(ArtifactError::Io(format!("subject not found: {display}")));
    }

    let candidates: &[(&str, &[&str])] = if cfg!(windows) {
        &[("certutil", &["-hashfile"])]
    } else {
        &[("sha256sum", &[]), ("shasum", &["-a", "256"])]
    };

    let mut last_err: Option<String> = None;
    for (tool, pre_args) in candidates {
        if !tool_on_path(tool) {
            continue;
        }
        let mut cmd = Command::new(tool);
        cmd.args(pre_args.iter());
        cmd.arg(path.as_os_str());
        if cfg!(windows) {
            cmd.arg("SHA256");
        }
        match cmd.output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                if let Some(hex) = extract_sha256(&text) {
                    return Ok(hex);
                }
                last_err = Some(format!("could not parse {tool} output"));
            }
            Ok(out) => {
                last_err = Some(format!(
                    "{tool} failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            Err(e) => {
                last_err = Some(format!("{tool} error: {e}"));
            }
        }
    }

    Err(ArtifactError::Digest(last_err.unwrap_or_else(|| {
        "no sha256 tool available on PATH".to_string()
    })))
}

/// Extracts the first 64-character lowercase hex token from digest tool output.
fn extract_sha256(text: &str) -> Option<String> {
    for token in text.split(|c: char| c.is_whitespace()) {
        let clean = token.trim();
        if clean.len() == 64 && clean.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(clean.to_ascii_lowercase());
        }
    }
    None
}

/// Reports whether a tool resolves on the current `PATH`.
fn tool_on_path(tool: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    for dir in env::split_paths(&paths) {
        let candidate = dir.join(tool);
        if candidate.is_file() {
            return true;
        }
        if cfg!(windows) {
            let exe = dir.join(format!("{tool}.exe"));
            if exe.is_file() {
                return true;
            }
        }
    }
    false
}

/// Builds the in-toto Statement value, reading builder and epoch from the environment.
fn build_statement(ctx: &Context, subjects: &[PathBuf]) -> Result<Value, ArtifactError> {
    build_statement_with(ctx, subjects, &builder_id(), source_date_epoch())
}

/// Builds the in-toto Statement value for the given subjects and context.
///
/// `builder` is the recorded builder identity and `epoch` is the optional
/// `SOURCE_DATE_EPOCH`-derived timestamp; both are passed explicitly to keep the
/// statement construction deterministic and free of ambient state.
fn build_statement_with(
    ctx: &Context,
    subjects: &[PathBuf],
    builder: &str,
    epoch: Option<i64>,
) -> Result<Value, ArtifactError> {
    let mut subject_entries = Vec::with_capacity(subjects.len());
    for subj in subjects {
        let digest = sha256_file(subj)?;
        let name = subj
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| subj.display().to_string());
        subject_entries.push(json!({
            "name": name,
            "digest": { "sha256": digest },
        }));
    }

    let mut sorted = ctx.repos.clone();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut materials: Vec<Value> = Vec::with_capacity(sorted.len());
    for repo in &sorted {
        materials.push(json!({
            "uri": format!("git+file://{}", repo.path),
            "name": repo.name,
            "digest": { "sha1": repo.rev },
            "groups": repo.groups,
        }));
    }

    let mut predicate = serde_json::Map::new();
    predicate.insert(
        "buildDefinition".to_string(),
        json!({
            "buildType": "https://haw.dev/provenance/repos/v1",
            "externalParameters": {
                "root": ctx.root,
                "stack": ctx.stack,
            },
            "resolvedDependencies": materials,
        }),
    );

    let mut run_details = serde_json::Map::new();
    run_details.insert("builder".to_string(), json!({ "id": builder }));
    let mut metadata = serde_json::Map::new();
    if let Some(epoch) = epoch {
        metadata.insert("startedOn".to_string(), json!(epoch));
        metadata.insert("finishedOn".to_string(), json!(epoch));
    }
    run_details.insert("metadata".to_string(), Value::Object(metadata));
    predicate.insert("runDetails".to_string(), Value::Object(run_details));

    Ok(json!({
        "_type": STATEMENT_TYPE,
        "subject": subject_entries,
        "predicateType": PREDICATE_TYPE,
        "predicate": Value::Object(predicate),
    }))
}

/// Serializes a JSON value deterministically (sorted keys, trailing newline).
fn to_deterministic_json(value: &Value) -> Result<String, ArtifactError> {
    let normalized = sort_value(value);
    let mut text = serde_json::to_string_pretty(&normalized)
        .map_err(|e| ArtifactError::Parse(format!("serializing statement: {e}")))?;
    text.push('\n');
    Ok(text)
}

/// Recursively rewrites JSON objects into key-sorted maps for stable output.
fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), sort_value(v)))
                .collect();
            match serde_json::to_value(sorted) {
                Ok(v) => v,
                Err(_) => value.clone(),
            }
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

/// Generates the provenance statement, writes outputs, and optionally signs.
fn generate(ctx: &Context, cli: &Cli) -> Result<Report, ArtifactError> {
    let mut report = Report {
        ok: true,
        ..Report::default()
    };

    let out_dir = cli
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUT));

    if cli.subjects.is_empty() {
        report.findings.push(Finding::warn(
            "no --subject given; statement has no subjects",
        ));
    }

    let statement = build_statement(ctx, &cli.subjects)?;
    let statement_text = to_deterministic_json(&statement)?;

    std::fs::create_dir_all(&out_dir)
        .map_err(|e| ArtifactError::Io(format!("creating {}: {e}", out_dir.display())))?;
    let prov_path = out_dir.join("provenance.json");
    std::fs::write(&prov_path, statement_text.as_bytes())
        .map_err(|e| ArtifactError::Io(format!("writing {}: {e}", prov_path.display())))?;
    report.artifacts.push(Artifact {
        path: prov_path.display().to_string(),
        kind: "provenance".to_string(),
    });

    if cli.sign {
        match sign_provenance(&prov_path) {
            Ok(Some(sig_path)) => {
                report.artifacts.push(Artifact {
                    path: sig_path.display().to_string(),
                    kind: "signature".to_string(),
                });
                report
                    .findings
                    .push(Finding::info(format!("signed via {}", sig_path.display())));
            }
            Ok(None) => {
                report
                    .findings
                    .push(Finding::warn("no signer on PATH; provenance unsigned"));
            }
            Err(e) => {
                report.findings.push(Finding::warn(format!(
                    "signing failed; provenance unsigned: {e}"
                )));
            }
        }
    }

    report.summary = format!(
        "provenance for {} subject(s) over {} material(s)",
        cli.subjects.len(),
        ctx.repos.len()
    );
    Ok(report)
}

/// Attempts to sign the provenance file with `cosign` or `minisign`.
///
/// Returns `Ok(Some(path))` with the signature artifact on success, `Ok(None)` when no
/// signer is available on `PATH`, or an error when a signer ran but failed.
fn sign_provenance(prov_path: &Path) -> Result<Option<PathBuf>, ArtifactError> {
    let sig_path = append_ext(prov_path, "sig");

    if tool_on_path("cosign") {
        let out = Command::new("cosign")
            .arg("sign-blob")
            .arg("--yes")
            .arg("--output-signature")
            .arg(sig_path.as_os_str())
            .arg(prov_path.as_os_str())
            .output()
            .map_err(|e| ArtifactError::Io(format!("running cosign: {e}")))?;
        if out.status.success() {
            return Ok(Some(sig_path));
        }
        return Err(ArtifactError::Io(format!(
            "cosign: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    if tool_on_path("minisign") {
        let out = Command::new("minisign")
            .arg("-S")
            .arg("-m")
            .arg(prov_path.as_os_str())
            .arg("-x")
            .arg(sig_path.as_os_str())
            .output()
            .map_err(|e| ArtifactError::Io(format!("running minisign: {e}")))?;
        if out.status.success() {
            return Ok(Some(sig_path));
        }
        return Err(ArtifactError::Io(format!(
            "minisign: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    Ok(None)
}

/// Appends an extension component to a path (e.g. `x.json` -> `x.json.sig`).
fn append_ext(path: &Path, ext: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    name.push('.');
    name.push_str(ext);
    path.with_file_name(name)
}

/// Verifies an existing provenance statement against the current context and disk.
///
/// This is the fail-closed admission gate: material SHAs must match the pinned context
/// revisions and every subject digest must match the file currently on disk.
fn verify(ctx: &Context, prov_path: &Path) -> Result<Report, ArtifactError> {
    let mut report = Report {
        ok: true,
        ..Report::default()
    };

    let raw = std::fs::read_to_string(prov_path)
        .map_err(|e| ArtifactError::Io(format!("reading {}: {e}", prov_path.display())))?;
    let statement: Value = serde_json::from_str(&raw)
        .map_err(|e| ArtifactError::Parse(format!("invalid provenance JSON: {e}")))?;

    let expected: BTreeMap<String, String> = ctx
        .repos
        .iter()
        .map(|r| (r.name.clone(), r.rev.clone()))
        .collect();

    let materials = statement
        .pointer("/predicate/buildDefinition/resolvedDependencies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut checked_materials = 0usize;
    for material in &materials {
        let name = material.get("name").and_then(Value::as_str).unwrap_or("");
        let recorded = material
            .pointer("/digest/sha1")
            .and_then(Value::as_str)
            .unwrap_or("");
        match expected.get(name) {
            Some(current) if current == recorded => {
                checked_materials += 1;
            }
            Some(current) => {
                report.ok = false;
                report.findings.push(Finding::error(format!(
                    "material '{name}' revision mismatch: statement {recorded}, context {current}"
                )));
            }
            None => {
                report.ok = false;
                report.findings.push(Finding::error(format!(
                    "material '{name}' not present in current context"
                )));
            }
        }
    }

    for name in expected.keys() {
        if !materials
            .iter()
            .any(|m| m.get("name").and_then(Value::as_str) == Some(name.as_str()))
        {
            report.ok = false;
            report.findings.push(Finding::error(format!(
                "context material '{name}' missing from statement"
            )));
        }
    }

    let subjects = statement
        .get("subject")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let base = prov_path.parent().unwrap_or_else(|| Path::new("."));
    let mut checked_subjects = 0usize;
    for subject in &subjects {
        let name = subject.get("name").and_then(Value::as_str).unwrap_or("");
        let recorded = subject
            .pointer("/digest/sha256")
            .and_then(Value::as_str)
            .unwrap_or("");
        match resolve_subject(base, name) {
            Some(file) => match sha256_file(&file) {
                Ok(actual) if actual == recorded => {
                    checked_subjects += 1;
                }
                Ok(actual) => {
                    report.ok = false;
                    report.findings.push(Finding::error(format!(
                        "subject '{name}' digest mismatch: statement {recorded}, disk {actual}"
                    )));
                }
                Err(e) => {
                    report.ok = false;
                    report
                        .findings
                        .push(Finding::error(format!("subject '{name}': {e}")));
                }
            },
            None => {
                report.ok = false;
                report.findings.push(Finding::error(format!(
                    "subject '{name}' not found on disk for verification"
                )));
            }
        }
    }

    report.summary = if report.ok {
        format!("verified {checked_subjects} subject(s) and {checked_materials} material(s)")
    } else {
        "verification failed".to_string()
    };
    Ok(report)
}

/// Resolves a recorded subject name to a file on disk, relative to the statement dir.
fn resolve_subject(base: &Path, name: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(name);
    if direct.is_file() {
        return Some(direct);
    }
    let rel = base.join(name);
    if rel.is_file() {
        return Some(rel);
    }
    None
}

/// Prints the report as `haw.plugin.report/1` JSON to stdout.
fn print_report(report: &Report, phase: Option<&str>) {
    let value = report.to_json(phase);
    match serde_json::to_string_pretty(&value) {
        Ok(text) => println!("{text}"),
        Err(_) => println!(
            "{{\"schema\":\"{REPORT_SCHEMA}\",\"plugin\":\"{PLUGIN_NAME}\",\"ok\":false,\"summary\":\"failed to serialize report\"}}"
        ),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn fake_context() -> Context {
        Context {
            root: "/root".to_string(),
            stack: Some("default".to_string()),
            repos: vec![
                Repo {
                    name: "beta".to_string(),
                    path: "/repos/beta".to_string(),
                    rev: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                    groups: vec!["core".to_string()],
                },
                Repo {
                    name: "alpha".to_string(),
                    path: "/repos/alpha".to_string(),
                    rev: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    groups: vec![],
                },
            ],
        }
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).expect("create temp file");
        f.write_all(bytes).expect("write temp file");
        path
    }

    #[test]
    fn parse_context_rejects_wrong_schema() {
        let raw = r#"{"schema":"other/1","root":"/x","repos":[]}"#;
        assert!(parse_context(raw).is_err());
    }

    #[test]
    fn parse_context_accepts_contract() {
        let raw = r#"{"schema":"haw.plugin/1","root":"/x","stack":null,
            "repos":[{"name":"a","path":"/a","rev":"deadbeef","groups":["g"]}]}"#;
        let ctx = parse_context(raw).expect("parse");
        assert_eq!(ctx.root, "/x");
        assert_eq!(ctx.repos.len(), 1);
        assert_eq!(ctx.repos[0].rev, "deadbeef");
    }

    #[test]
    fn statement_binds_subject_and_all_materials() {
        let dir = tempdir().expect("tempdir");
        let subject = write_file(dir.path(), "artifact.bin", b"hello haw");
        let ctx = fake_context();

        let stmt = build_statement(&ctx, std::slice::from_ref(&subject)).expect("statement");

        assert_eq!(stmt["_type"], STATEMENT_TYPE);
        assert_eq!(stmt["predicateType"], PREDICATE_TYPE);

        let subj_digest = stmt["subject"][0]["digest"]["sha256"]
            .as_str()
            .expect("subject digest");
        let expected = sha256_file(&subject).expect("digest");
        assert_eq!(subj_digest, expected);
        assert_eq!(subj_digest.len(), 64);

        let materials = stmt
            .pointer("/predicate/buildDefinition/resolvedDependencies")
            .and_then(Value::as_array)
            .expect("materials");
        assert_eq!(materials.len(), 2);
        for repo in &ctx.repos {
            let found = materials.iter().any(|m| {
                m["name"].as_str() == Some(repo.name.as_str())
                    && m["digest"]["sha1"].as_str() == Some(repo.rev.as_str())
            });
            assert!(found, "material {} with its rev present", repo.name);
        }
    }

    #[test]
    fn statement_is_valid_json_and_deterministic() {
        let dir = tempdir().expect("tempdir");
        let subject = write_file(dir.path(), "artifact.bin", b"determinism");
        let ctx = fake_context();
        let subjects = std::slice::from_ref(&subject);

        let first = to_deterministic_json(
            &build_statement_with(&ctx, subjects, "haw", Some(1_700_000_000)).expect("s1"),
        )
        .expect("json1");
        let second = to_deterministic_json(
            &build_statement_with(&ctx, subjects, "haw", Some(1_700_000_000)).expect("s2"),
        )
        .expect("json2");

        let parsed: Value = serde_json::from_str(&first).expect("valid json");
        assert_eq!(parsed["_type"], STATEMENT_TYPE);
        assert_eq!(first, second, "output must be byte-identical across runs");
    }

    #[test]
    fn verify_detects_material_mismatch() {
        let dir = tempdir().expect("tempdir");
        let subject = write_file(dir.path(), "artifact.bin", b"content");
        let mut ctx = fake_context();

        let stmt = build_statement(&ctx, &[subject]).expect("statement");
        let text = to_deterministic_json(&stmt).expect("json");
        let prov = dir.path().join("provenance.json");
        std::fs::write(&prov, text).expect("write prov");

        ctx.repos[0].rev = "ffffffffffffffffffffffffffffffffffffffff".to_string();

        let report = verify(&ctx, &prov).expect("verify runs");
        assert!(!report.ok, "verify must fail-closed on material mismatch");
        assert!(
            report.findings.iter().any(|f| f.severity == "error"),
            "expected an error finding"
        );
    }

    #[test]
    fn verify_detects_subject_digest_mismatch() {
        let dir = tempdir().expect("tempdir");
        let subject = write_file(dir.path(), "artifact.bin", b"original");
        let ctx = fake_context();

        let stmt = build_statement(&ctx, &[subject]).expect("statement");
        let text = to_deterministic_json(&stmt).expect("json");
        let prov = dir.path().join("provenance.json");
        std::fs::write(&prov, text).expect("write prov");

        write_file(dir.path(), "artifact.bin", b"tampered!!");

        let report = verify(&ctx, &prov).expect("verify runs");
        assert!(!report.ok, "verify must fail-closed on subject mismatch");
    }

    #[test]
    fn verify_passes_when_consistent() {
        let dir = tempdir().expect("tempdir");
        let subject = write_file(dir.path(), "artifact.bin", b"stable");
        let ctx = fake_context();

        let stmt = build_statement(&ctx, &[subject]).expect("statement");
        let text = to_deterministic_json(&stmt).expect("json");
        let prov = dir.path().join("provenance.json");
        std::fs::write(&prov, text).expect("write prov");

        let report = verify(&ctx, &prov).expect("verify runs");
        assert!(report.ok, "verify should pass when nothing changed");
    }

    #[test]
    fn extract_sha256_parses_shasum_style() {
        let line = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  file";
        assert_eq!(
            extract_sha256(line).as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }
}
