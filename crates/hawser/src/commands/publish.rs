//! `haw publish` command handlers: config resolution, dry-run, and upload.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use haw_core::workspace::Workspace;
use serde_json::json;

use crate::publish;
use crate::ui::palette::Palette;
use crate::{git_capture, open_workspace};

/// Config resolved from the environment for one publish target: the base URL,
/// the credentials, and the target-specific `repo`/`project_id` path parts.
struct PublishConfig {
    base: String,
    repo: String,
    project_id: String,
    auth: publish::Auth,
}

/// Read a non-empty env var, `None` if unset or blank.
fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// Resolve the credentials/URL for `target` from the environment.
///
/// `url_override` (from `--url`) wins over the env base URL. When required
/// creds are absent, returns an `Err` listing the env vars needed — never
/// panics. `--dry-run` short-circuits before this is called with real creds,
/// so a missing-cred error only ever surfaces on a real upload.
fn resolve_publish_config(
    target: publish::Target,
    url_override: Option<&str>,
) -> Result<PublishConfig> {
    use publish::{Auth, Target};
    match target {
        Target::Nexus => {
            let base = url_override
                .map(str::to_string)
                .or_else(|| env_nonempty("NEXUS_URL"))
                .context(
                    "nexus needs a base URL: set NEXUS_URL (with NEXUS_USER, NEXUS_PASS) or pass --url",
                )?;
            let user = env_nonempty("NEXUS_USER")
                .context("nexus needs NEXUS_USER and NEXUS_PASS in the environment")?;
            let pass = env_nonempty("NEXUS_PASS")
                .context("nexus needs NEXUS_USER and NEXUS_PASS in the environment")?;
            Ok(PublishConfig {
                base,
                repo: env_nonempty("NEXUS_REPO").unwrap_or_else(|| "raw-hosted".to_string()),
                project_id: String::new(),
                auth: Auth::Basic { user, pass },
            })
        }
        Target::Artifactory => {
            let base = url_override
                .map(str::to_string)
                .or_else(|| env_nonempty("ARTIFACTORY_URL"))
                .context(
                    "artifactory needs a base URL: set ARTIFACTORY_URL (with ARTIFACTORY_TOKEN) or pass --url",
                )?;
            let token = env_nonempty("ARTIFACTORY_TOKEN")
                .context("artifactory needs ARTIFACTORY_TOKEN in the environment")?;
            Ok(PublishConfig {
                base,
                repo: env_nonempty("ARTIFACTORY_REPO")
                    .unwrap_or_else(|| "generic-local".to_string()),
                project_id: String::new(),
                auth: Auth::Bearer(token),
            })
        }
        Target::GitLab => {
            let base = url_override
                .map(str::to_string)
                .or_else(|| env_nonempty("GITLAB_URL"))
                .unwrap_or_else(|| "https://gitlab.com".to_string());
            let token = env_nonempty("GITLAB_TOKEN")
                .context("gitlab needs GITLAB_TOKEN and GITLAB_PROJECT_ID in the environment")?;
            let project_id = env_nonempty("GITLAB_PROJECT_ID")
                .context("gitlab needs GITLAB_TOKEN and GITLAB_PROJECT_ID in the environment")?;
            Ok(PublishConfig {
                base,
                repo: String::new(),
                project_id,
                auth: Auth::PrivateToken(token),
            })
        }
        Target::Bitbucket => {
            let base = url_override
                .map(str::to_string)
                .unwrap_or_else(|| "https://api.bitbucket.org".to_string());
            let user = env_nonempty("BITBUCKET_USER").context(
                "bitbucket needs BITBUCKET_USER, BITBUCKET_TOKEN, BITBUCKET_WORKSPACE, BITBUCKET_REPO",
            )?;
            let pass = env_nonempty("BITBUCKET_TOKEN").context(
                "bitbucket needs BITBUCKET_USER, BITBUCKET_TOKEN, BITBUCKET_WORKSPACE, BITBUCKET_REPO",
            )?;
            let workspace = env_nonempty("BITBUCKET_WORKSPACE").context(
                "bitbucket needs BITBUCKET_USER, BITBUCKET_TOKEN, BITBUCKET_WORKSPACE, BITBUCKET_REPO",
            )?;
            let repo = env_nonempty("BITBUCKET_REPO").context(
                "bitbucket needs BITBUCKET_USER, BITBUCKET_TOKEN, BITBUCKET_WORKSPACE, BITBUCKET_REPO",
            )?;
            Ok(PublishConfig {
                base,
                repo: format!("{workspace}/{repo}"),
                project_id: String::new(),
                auth: Auth::Basic { user, pass },
            })
        }
    }
}

/// The base URL each target reads from the environment (for the dry-run plan
/// when no creds are present and `--url` was not passed). Placeholders keep the
/// printed URL readable so users see exactly which var feeds which slot.
fn dry_run_base(target: publish::Target, url_override: Option<&str>) -> String {
    use publish::Target;
    if let Some(url) = url_override {
        return url.to_string();
    }
    match target {
        Target::Nexus => env_nonempty("NEXUS_URL").unwrap_or_else(|| "$NEXUS_URL".to_string()),
        Target::Artifactory => {
            env_nonempty("ARTIFACTORY_URL").unwrap_or_else(|| "$ARTIFACTORY_URL".to_string())
        }
        Target::GitLab => {
            env_nonempty("GITLAB_URL").unwrap_or_else(|| "https://gitlab.com".to_string())
        }
        Target::Bitbucket => "https://api.bitbucket.org".to_string(),
    }
}

/// The `repo`/`project_id` path parts for the dry-run plan, using env values
/// where set and readable placeholders otherwise.
fn dry_run_parts(target: publish::Target) -> (String, String) {
    use publish::Target;
    match target {
        Target::Nexus => (
            env_nonempty("NEXUS_REPO").unwrap_or_else(|| "raw-hosted".to_string()),
            String::new(),
        ),
        Target::Artifactory => (
            env_nonempty("ARTIFACTORY_REPO").unwrap_or_else(|| "generic-local".to_string()),
            String::new(),
        ),
        Target::GitLab => (
            String::new(),
            env_nonempty("GITLAB_PROJECT_ID").unwrap_or_else(|| "$GITLAB_PROJECT_ID".to_string()),
        ),
        Target::Bitbucket => {
            let ws = env_nonempty("BITBUCKET_WORKSPACE")
                .unwrap_or_else(|| "$BITBUCKET_WORKSPACE".to_string());
            let repo =
                env_nonempty("BITBUCKET_REPO").unwrap_or_else(|| "$BITBUCKET_REPO".to_string());
            (format!("{ws}/{repo}"), String::new())
        }
    }
}

/// The auth scheme label each target uses, for the dry-run plan (no secrets).
fn dry_run_auth(target: publish::Target) -> publish::Auth {
    use publish::{Auth, Target};
    match target {
        Target::Nexus | Target::Bitbucket => Auth::Basic {
            user: String::new(),
            pass: String::new(),
        },
        Target::Artifactory => Auth::Bearer(String::new()),
        Target::GitLab => Auth::PrivateToken(String::new()),
    }
}

/// The base file name (final path component) for use in the upload URL.
fn upload_file_name(path: &Path) -> Result<String> {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .with_context(|| format!("{} has no file name", path.display()))
}

/// Expand the positional `files` (paths or globs) to concrete existing files.
///
/// With no arguments, defaults to `haw-evidence.tar.gz` if it exists, else a
/// clear error pointing at `haw evidence`. A glob that matches nothing but
/// contains glob metacharacters is an error; a plain path that does not exist
/// is also an error.
fn resolve_publish_files(files: &[String]) -> Result<Vec<PathBuf>> {
    if files.is_empty() {
        let evidence = PathBuf::from("haw-evidence.tar.gz");
        if evidence.exists() {
            return Ok(vec![evidence]);
        }
        bail!(
            "no files to publish and haw-evidence.tar.gz not found — pass files, \
             or run `haw evidence` first to build the bundle"
        );
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for pattern in files {
        if pattern.contains(['*', '?', '[']) {
            let mut matched = 0usize;
            for entry in glob_paths(pattern) {
                matched += 1;
                if !out.contains(&entry) {
                    out.push(entry);
                }
            }
            if matched == 0 {
                bail!("glob `{pattern}` matched no files");
            }
        } else {
            let path = PathBuf::from(pattern);
            if !path.exists() {
                bail!("{pattern} does not exist");
            }
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    Ok(out)
}

/// Minimal single-level glob for the current directory / a fixed dir prefix.
/// Supports `*` and `?` in the final path component (e.g. `out/*.bin`). Enough
/// for the common `./out/*.bin` publish case without a new dependency.
fn glob_paths(pattern: &str) -> Vec<PathBuf> {
    let p = Path::new(pattern);
    let Some(file_glob) = p.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return Vec::new();
    };
    let dir = p.parent().filter(|d| !d.as_os_str().is_empty());
    let read_dir = match dir {
        Some(d) => std::fs::read_dir(d),
        None => std::fs::read_dir("."),
    };
    let mut matches = Vec::new();
    if let Ok(entries) = read_dir {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if glob_match(&file_glob, &name) {
                matches.push(entry.path());
            }
        }
    }
    matches.sort();
    matches
}

/// Match a single path component against a `*`/`?` glob.
fn glob_match(pattern: &str, text: &str) -> bool {
    fn helper(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => helper(&p[1..], t) || (!t.is_empty() && helper(p, &t[1..])),
            Some('?') => !t.is_empty() && helper(&p[1..], &t[1..]),
            Some(c) => !t.is_empty() && *c == t[0] && helper(&p[1..], &t[1..]),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    helper(&p, &t)
}

/// Default package version: the short HEAD SHA of the manifest repo, else
/// `unversioned`. Best-effort — never fails the command.
fn default_publish_version(root: &Path) -> String {
    let sha = git_capture(root, &["rev-parse", "--short", "HEAD"]);
    let sha = sha.trim();
    if sha.is_empty() || sha.contains(char::is_whitespace) {
        "unversioned".to_string()
    } else {
        sha.to_string()
    }
}

/// Default package name: the current stack, else the workspace directory name,
/// else `fleet`.
fn default_publish_name(ws: &Workspace) -> String {
    if let Some(stack) = ws.current_stack() {
        return stack;
    }
    ws.root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "fleet".to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn publish_cmd(
    files: &[String],
    to: &str,
    name: Option<&str>,
    version: Option<&str>,
    url: Option<&str>,
    dry_run: bool,
    insecure: bool,
    format: Option<&str>,
) -> Result<ExitCode> {
    let json = match format {
        None | Some("text") => false,
        Some("json") => true,
        Some(other) => bail!("unknown format `{other}` (use text or json)"),
    };
    let target = publish::Target::parse(to).map_err(|e| anyhow::anyhow!(e))?;
    let paths = resolve_publish_files(files)?;

    // Name/version defaults derive from the workspace when one is present, but
    // publish must also work standalone (e.g. a CI job with just artifacts).
    let ws = open_workspace().ok();
    let name = match name {
        Some(n) => n.to_string(),
        None => ws
            .as_ref()
            .map(default_publish_name)
            .unwrap_or_else(|| "fleet".to_string()),
    };
    let version = match version {
        Some(v) => v.to_string(),
        None => {
            let root = ws
                .as_ref()
                .map(|w| w.root.clone())
                .unwrap_or_else(|| PathBuf::from("."));
            default_publish_version(&root)
        }
    };

    if dry_run {
        return publish_dry_run(target, url, &name, &version, &paths, json);
    }

    let config = resolve_publish_config(target, url)?;
    // Refuse to send upload credentials over cleartext http:// (accidental leak /
    // MITM). Internal https registries on private IPs are fine — only the scheme
    // is gated. `--insecure` overrides for a deliberately-plaintext registry.
    if !insecure && config.base.starts_with("http://") {
        bail!(
            "publish target `{}` is http:// — credentials would be sent in cleartext.\n\
             Use an https:// registry, or pass --insecure to allow it.",
            config.base
        );
    }
    // Bound redirects so a hostile/misconfigured registry can't bounce an
    // authenticated upload (with its credentials) off to an arbitrary host.
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .context("building HTTP client")?;
    let c = Palette::new();
    let mut uploads: Vec<serde_json::Value> = Vec::new();
    let mut failures = 0usize;

    if !json {
        println!(
            "{}",
            c.bold(&format!(
                "publishing {} file(s) to {} as {}/{}",
                paths.len(),
                target.as_str(),
                name,
                version
            ))
        );
    }

    for path in &paths {
        let file = upload_file_name(path)?;
        let plan = publish::plan_upload(
            target,
            &config.base,
            &config.repo,
            &config.project_id,
            &name,
            &version,
            &file,
            config.auth.clone(),
        );
        let result = upload_one(&client, &plan, path);
        match result {
            Ok(status) if (200..300).contains(&status) => {
                if !json {
                    println!("  {} {}  {} {status}", c.ok("✓"), c.name(&file), c.dim("→"));
                }
                uploads.push(json!({"file": file, "url": plan.url, "status": status}));
            }
            Ok(status) => {
                failures += 1;
                if !json {
                    eprintln!("  {} {}  HTTP {status}", c.err("✗"), file);
                }
                uploads.push(json!({"file": file, "url": plan.url, "status": status}));
            }
            Err(err) => {
                failures += 1;
                if !json {
                    eprintln!("  {} {}  {err}", c.err("✗"), file);
                }
                uploads.push(
                    json!({"file": file, "url": plan.url, "status": serde_json::Value::Null, "error": err.to_string()}),
                );
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": "haw.publish/1",
                "target": target.as_str(),
                "name": name,
                "version": version,
                "uploads": uploads,
            }))?
        );
    } else {
        println!(
            "{}",
            c.bold(&format!(
                "uploaded {}/{} file(s)",
                paths.len() - failures,
                paths.len()
            ))
        );
    }

    if failures > 0 {
        bail!("{failures} file(s) failed to upload to {}", target.as_str());
    }
    Ok(ExitCode::SUCCESS)
}

/// Build and send one upload request from its plan. Returns the HTTP status.
fn upload_one(
    client: &reqwest::blocking::Client,
    plan: &publish::UploadPlan,
    path: &Path,
) -> Result<u16> {
    use publish::{Auth, Method};
    let req = match plan.method {
        Method::Put => {
            let body =
                std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            client.put(&plan.url).body(body)
        }
        Method::PostMultipart => {
            let form = reqwest::blocking::multipart::Form::new()
                .file("files", path)
                .with_context(|| format!("attaching {}", path.display()))?;
            client.post(&plan.url).multipart(form)
        }
    };
    let req = match &plan.auth {
        Auth::Basic { user, pass } => req.basic_auth(user, Some(pass)),
        Auth::Bearer(token) => req.bearer_auth(token),
        Auth::PrivateToken(token) => req.header("PRIVATE-TOKEN", token),
    };
    let resp = req
        .send()
        .with_context(|| format!("uploading {} to {}", plan.file, plan.url))?;
    Ok(resp.status().as_u16())
}

/// Render the plan `haw publish` WOULD execute, without touching the network.
fn publish_dry_run(
    target: publish::Target,
    url: Option<&str>,
    name: &str,
    version: &str,
    paths: &[PathBuf],
    json: bool,
) -> Result<ExitCode> {
    let base = dry_run_base(target, url);
    let (repo, project_id) = dry_run_parts(target);
    let auth = dry_run_auth(target);

    let mut plans = Vec::with_capacity(paths.len());
    for path in paths {
        let file = upload_file_name(path)?;
        plans.push(publish::plan_upload(
            target,
            &base,
            &repo,
            &project_id,
            name,
            version,
            &file,
            auth.clone(),
        ));
    }

    if json {
        let uploads: Vec<serde_json::Value> = plans
            .iter()
            .map(|p| {
                json!({
                    "file": p.file,
                    "method": p.method.as_str(),
                    "url": p.url,
                    "auth": p.auth.scheme(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": "haw.publish/1",
                "dry_run": true,
                "target": target.as_str(),
                "name": name,
                "version": version,
                "uploads": uploads,
            }))?
        );
        return Ok(ExitCode::SUCCESS);
    }

    let c = Palette::new();
    println!(
        "{}",
        c.bold(&format!(
            "dry run: would publish {} file(s) to {} as {}/{}",
            plans.len(),
            target.as_str(),
            name,
            version
        ))
    );
    for plan in &plans {
        println!(
            "  {} {}  {} {}  {} {}",
            c.dim(plan.method.as_str()),
            c.name(&plan.file),
            c.dim("→"),
            c.rev(&plan.url),
            c.dim("auth:"),
            plan.auth.scheme(),
        );
    }
    println!("{}", c.dim("(no network — remove --dry-run to upload)"));
    Ok(ExitCode::SUCCESS)
}
