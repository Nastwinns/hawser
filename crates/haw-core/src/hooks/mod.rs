//! Lifecycle hooks: scripts in `.haw/hooks/` fired around haw operations.
//! Context arrives via env (`HAW_ROOT`, `HAW_HOOK`) and JSON on stdin.
//! A failing `pre-*` hook aborts the operation; `post-*` failures surface
//! as errors the caller may downgrade to warnings.

use std::io::Write;
use std::path::PathBuf;

use crate::workspace::Workspace;

/// The lifecycle points a workspace can hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    PreSync,
    PostSync,
    PreLock,
    PostLock,
    PostSwitch,
    PostChangeStart,
    PreBuild,
    PostBuild,
    PreTest,
    PostTest,
    PreRequest,
    PostLand,
}

impl Hook {
    /// The kebab-case name used for the hook script, the `HAW_HOOK` env var,
    /// and the `--haw-phase` argument passed to subscribed plugins.
    pub fn name(self) -> &'static str {
        match self {
            Hook::PreSync => "pre-sync",
            Hook::PostSync => "post-sync",
            Hook::PreLock => "pre-lock",
            Hook::PostLock => "post-lock",
            Hook::PostSwitch => "post-switch",
            Hook::PostChangeStart => "post-change-start",
            Hook::PreBuild => "pre-build",
            Hook::PostBuild => "post-build",
            Hook::PreTest => "pre-test",
            Hook::PostTest => "post-test",
            Hook::PreRequest => "pre-request",
            Hook::PostLand => "post-land",
        }
    }

    /// Every lifecycle hook, in declaration order. Used by the manifest to
    /// validate `[plugins]` phase subscriptions and by `haw hooks list`.
    pub const ALL: [Hook; 12] = [
        Hook::PreSync,
        Hook::PostSync,
        Hook::PreLock,
        Hook::PostLock,
        Hook::PostSwitch,
        Hook::PostChangeStart,
        Hook::PreBuild,
        Hook::PostBuild,
        Hook::PreTest,
        Hook::PostTest,
        Hook::PreRequest,
        Hook::PostLand,
    ];

    /// Whether this is a `pre-*` hook (its failure may abort the operation).
    pub fn is_pre(self) -> bool {
        self.name().starts_with("pre-")
    }
}

/// Errors from a hook run.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("hook `{hook}` could not run")]
    Spawn {
        hook: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("hook `{hook}` failed with {status}")]
    Failed { hook: &'static str, status: String },
}

fn script_path(ws: &Workspace, hook: Hook) -> PathBuf {
    let dir = ws.state_dir().join("hooks");
    if cfg!(windows) {
        let bat = dir.join(format!("{}.bat", hook.name()));
        if bat.exists() {
            return bat;
        }
    }
    dir.join(hook.name())
}

/// Run the hook if a script exists; silently succeed otherwise.
pub fn fire(ws: &Workspace, hook: Hook, context: &serde_json::Value) -> Result<(), HookError> {
    let script = script_path(ws, hook);
    if !script.exists() {
        return Ok(());
    }
    let mut child = std::process::Command::new(&script)
        .current_dir(&ws.root)
        .env("HAW_ROOT", &ws.root)
        .env("HAW_HOOK", hook.name())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|source| HookError::Spawn {
            hook: hook.name(),
            source,
        })?;
    if let Some(stdin) = child.stdin.take() {
        let mut stdin = stdin;
        let _ = stdin.write_all(context.to_string().as_bytes());
    }
    let status = child.wait().map_err(|source| HookError::Spawn {
        hook: hook.name(),
        source,
    })?;
    if !status.success() {
        return Err(HookError::Failed {
            hook: hook.name(),
            status: status.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_hooks_have_kebab_names() {
        let names: Vec<&str> = Hook::ALL.iter().map(|h| h.name()).collect();
        assert_eq!(names.len(), 12);
        assert!(names.contains(&"pre-build"));
        assert!(names.contains(&"post-build"));
        assert!(names.contains(&"pre-test"));
        assert!(names.contains(&"post-test"));
        assert!(names.contains(&"pre-request"));
        assert!(names.contains(&"post-land"));
        for name in &names {
            assert!(!name.contains('_'), "{name} should be kebab-case");
        }
    }

    #[test]
    fn hook_names_are_unique() {
        let mut names: Vec<&str> = Hook::ALL.iter().map(|h| h.name()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "hook names must be unique");
    }

    #[test]
    fn is_pre_matches_name_prefix() {
        assert!(Hook::PreBuild.is_pre());
        assert!(Hook::PreRequest.is_pre());
        assert!(!Hook::PostBuild.is_pre());
        assert!(!Hook::PostLand.is_pre());
    }
}
