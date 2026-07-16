//! The checks the gate runs against each repository.
//!
//! The gate is deliberately honest: it runs whatever real coverage is available
//! (`gitleaks`) and, when that is absent, falls back to a small set of
//! high-signal heuristics that are always clearly labelled as such. It never
//! reports full-confidence coverage it does not have.

use std::path::Path;
use std::process::Command;

use crate::report::Finding;

/// Whether `gitleaks` is available on `PATH`.
pub fn gitleaks_available() -> bool {
    tool_on_path("gitleaks")
}

/// Whether `git` is available on `PATH`.
fn git_available() -> bool {
    tool_on_path("git")
}

/// Probe `PATH` for `tool` by running `tool --version`.
fn tool_on_path(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run `gitleaks detect` in `repo` and return findings.
///
/// A nonzero exit from gitleaks means it found leaks; that becomes an
/// `error`-level finding. If gitleaks fails to run for an operational reason,
/// a `warn` is emitted so the absence of coverage is visible.
pub fn run_gitleaks(repo: &Path) -> Vec<Finding> {
    let output = Command::new("gitleaks")
        .arg("detect")
        .arg("--no-banner")
        .arg("--redact")
        .arg("--source")
        .arg(repo)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            vec![Finding::info(format!(
                "gitleaks: no secrets detected in {}",
                repo.display()
            ))]
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let detail = stderr.lines().last().unwrap_or("").trim();
            vec![Finding::error(format!(
                "gitleaks detected secrets in {}{}",
                repo.display(),
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(" ({detail})")
                }
            ))]
        }
        Err(e) => vec![Finding::warn(format!(
            "gitleaks failed to run on {}: {e}",
            repo.display()
        ))],
    }
}

/// A file to scan, addressed relative to the repo root.
struct TextFile {
    rel: String,
    contents: String,
}

/// Enumerate tracked text files in `repo` (via git), falling back to a shallow
/// filesystem walk when git is unavailable or the path is not a repo.
fn collect_files(repo: &Path) -> Vec<TextFile> {
    if git_available()
        && let Some(files) = collect_via_git(repo)
    {
        return files;
    }
    collect_via_walk(repo)
}

/// List tracked and staged files (plus untracked, non-ignored ones) with
/// `git ls-files` and read those that are valid UTF-8. Including staged and
/// untracked content is what makes this a pre-commit gate rather than a
/// history-only audit.
fn collect_via_git(repo: &Path) -> Option<Vec<TextFile>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("ls-files")
        .arg("-z")
        .arg("--cached")
        .arg("--others")
        .arg("--exclude-standard")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut files = Vec::new();
    for rel_bytes in output.stdout.split(|b| *b == 0) {
        if rel_bytes.is_empty() {
            continue;
        }
        let rel = String::from_utf8_lossy(rel_bytes).into_owned();
        let full = repo.join(&rel);
        if let Ok(contents) = std::fs::read_to_string(&full) {
            files.push(TextFile { rel, contents });
        }
    }
    Some(files)
}

/// Shallow recursive walk that reads UTF-8 files, skipping the `.git` dir.
fn collect_via_walk(repo: &Path) -> Vec<TextFile> {
    let mut files = Vec::new();
    walk(repo, repo, &mut files);
    files
}

/// Recursive helper for [`collect_via_walk`].
fn walk(root: &Path, dir: &Path, out: &mut Vec<TextFile>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => walk(root, &path, out),
            Ok(ft) if ft.is_file() => {
                if let Ok(contents) = std::fs::read_to_string(&path) {
                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned();
                    out.push(TextFile { rel, contents });
                }
            }
            _ => {}
        }
    }
}

/// Detect an AWS access key id: the literal `AKIA` followed by 16 uppercase
/// alphanumeric characters. Returns true if any such token appears in `line`.
fn has_aws_akia_key(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 20 <= bytes.len() {
        if &bytes[i..i + 4] == b"AKIA" {
            let tail = &bytes[i + 4..i + 20];
            if tail
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                let after_ok = bytes
                    .get(i + 20)
                    .map(|b| !(b.is_ascii_uppercase() || b.is_ascii_digit()))
                    .unwrap_or(true);
                if after_ok {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// Detect a PEM private-key header of the form `-----BEGIN <...>PRIVATE KEY-----`.
fn has_private_key_header(line: &str) -> bool {
    if let Some(start) = line.find("-----BEGIN ") {
        let rest = &line[start + "-----BEGIN ".len()..];
        if let Some(end) = rest.find("-----") {
            let label = &rest[..end];
            return label.contains("PRIVATE KEY");
        }
    }
    false
}

/// Run the labelled heuristic secret scan over tracked text in `repo`.
///
/// Each hit is a `warn` (heuristics are not authoritative). This function does
/// not itself emit the "install gitleaks" info notice; the caller adds that
/// whenever gitleaks is absent so the honesty guarantee holds even for clean
/// repos.
pub fn heuristic_secret_scan(repo: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in collect_files(repo) {
        for (idx, line) in file.contents.lines().enumerate() {
            let lineno = idx + 1;
            if has_aws_akia_key(line) {
                findings.push(Finding::warn(format!(
                    "heuristic: possible AWS access key id in {}:{lineno}",
                    file.rel
                )));
            }
            if has_private_key_header(line) {
                findings.push(Finding::warn(format!(
                    "heuristic: PEM private key header in {}:{lineno}",
                    file.rel
                )));
            }
        }
    }
    findings
}

/// Run cheap, honest formatting-hygiene checks over tracked text in `repo`.
///
/// Flags trailing whitespace and a missing final newline as `warn`.
pub fn hygiene_scan(repo: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in collect_files(repo) {
        for (idx, line) in file.contents.lines().enumerate() {
            if line.ends_with(' ') || line.ends_with('\t') {
                findings.push(Finding::warn(format!(
                    "hygiene: trailing whitespace in {}:{}",
                    file.rel,
                    idx + 1
                )));
            }
        }
        if !file.contents.is_empty() && !file.contents.ends_with('\n') {
            findings.push(Finding::warn(format!(
                "hygiene: missing final newline in {}",
                file.rel
            )));
        }
    }
    findings
}
