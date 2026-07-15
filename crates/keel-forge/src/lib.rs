//! PR/MR orchestration behind the [`Forge`] trait.
//!
//! Detection is live; the GitHub (octocrab) and GitLab implementations land
//! behind this trait in Phases 1 and 3.

/// Which forge a remote URL belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeKind {
    GitHub,
    GitLab,
    Unknown,
}

/// A PR/MR to open.
#[derive(Debug, Clone)]
pub struct PrSpec {
    pub title: String,
    pub body: String,
    pub source_branch: String,
    pub target_branch: String,
}

/// Handle to an opened PR/MR.
#[derive(Debug, Clone)]
pub struct PrHandle {
    pub url: String,
    pub number: u64,
}

/// Review/merge state of a PR/MR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Draft,
    Merged,
    Closed,
}

/// Aggregated PR/MR status for the dashboard.
#[derive(Debug, Clone)]
pub struct PrStatus {
    pub state: PrState,
    pub approved: bool,
    /// `None` while CI is pending or absent.
    pub ci_passing: Option<bool>,
    pub url: String,
}

/// Errors from a forge implementation.
#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    #[error("{0} support is not implemented yet")]
    NotImplemented(&'static str),
    #[error("forge API error: {0}")]
    Api(String),
}

/// One forge (GitHub, GitLab, ...) driving PR/MRs for a repository URL.
pub trait Forge {
    fn open_pr(&self, repo_url: &str, spec: &PrSpec) -> Result<PrHandle, ForgeError>;
    fn pr_status(&self, repo_url: &str, number: u64) -> Result<PrStatus, ForgeError>;
    fn merge_pr(&self, repo_url: &str, number: u64) -> Result<(), ForgeError>;
}

/// Host part of an HTTP(S), ssh://, or scp-like (`git@host:path`) git URL.
fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    if let Some((_, after_scheme)) = url.split_once("://") {
        let authority = after_scheme.split(['/', '?']).next()?;
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        return Some(host.split(':').next().unwrap_or(host));
    }
    if let Some((user_host, _path)) = rest.split_once(':')
        && let Some((_, host)) = user_host.rsplit_once('@')
    {
        return Some(host);
    }
    None
}

/// Guess the forge from a remote URL. Self-hosted instances are matched by
/// hostname substring; anything else needs explicit configuration later.
pub fn detect(url: &str) -> ForgeKind {
    match host_of(url) {
        Some(host) if host.contains("github") => ForgeKind::GitHub,
        Some(host) if host.contains("gitlab") => ForgeKind::GitLab,
        _ => ForgeKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{ForgeKind, detect};

    #[test]
    fn detects_github() {
        assert_eq!(detect("https://github.com/acme/x.git"), ForgeKind::GitHub);
        assert_eq!(detect("git@github.com:acme/x.git"), ForgeKind::GitHub);
        assert_eq!(
            detect("ssh://git@github.enterprise.local/acme/x.git"),
            ForgeKind::GitHub
        );
    }

    #[test]
    fn detects_gitlab_including_self_hosted() {
        assert_eq!(detect("https://gitlab.com/acme/x.git"), ForgeKind::GitLab);
        assert_eq!(
            detect("git@gitlab.company.com:firmware/kernel.git"),
            ForgeKind::GitLab
        );
    }

    #[test]
    fn unknown_for_everything_else() {
        assert_eq!(
            detect("https://bitbucket.org/acme/x.git"),
            ForgeKind::Unknown
        );
        assert_eq!(detect("/tmp/local/repo"), ForgeKind::Unknown);
        assert_eq!(detect("file:///tmp/local/repo"), ForgeKind::Unknown);
    }
}
