//! User-customizable cockpit configuration: `~/.config/haw/config.toml`.
//!
//! The file is entirely optional. A missing file, a partial `[ui]`/`[keys]`
//! table, or an unknown key all resolve to sane defaults — [`Config::load`]
//! never errors on absence and never panics. The pickers ([`crate::run`]) write
//! the file back via [`Config::save`] to persist a chosen theme/editor.
//!
//! ## Precedence
//!
//! Theme (see [`resolve_theme`]):
//!   1. `NO_COLOR` (non-empty env) → always `monochrome` (per the NO_COLOR spec)
//!   2. `HAW_THEME` (env, if it names a built-in)
//!   3. config `[ui].theme` (if it names a built-in)
//!   4. built-in default (`catppuccin`)
//!
//! Editor (see [`resolve_editor_with`]):
//!   1. `$VISUAL` (if set & non-empty)
//!   2. `$EDITOR` (if set & non-empty)
//!   3. config `[ui].editor` (if set & non-empty)
//!   4. first of `nvim`/`vim`/`vi` found on `PATH`
//!
//! Env always wins for both so a one-off `HAW_THEME=nord haw dash` or
//! `EDITOR=code haw dash` overrides the persisted config without touching it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Theme;

/// The parsed user config. Every field is optional at the file level (serde
/// `default`), so a truncated or partial TOML still deserializes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
    pub keys: KeyConfig,
}

/// The `[ui]` table: startup theme, editor, and display options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Startup theme name (a built-in; unknown names are ignored at resolve time).
    pub theme: Option<String>,
    /// Editor command for the `e` key (env still wins — see module docs).
    pub editor: Option<String>,
    /// Start with the header collapsed to a single compact line.
    pub compact_header: bool,
    /// Idle auto-refresh cadence in seconds (clamped to [`REFRESH_MIN`]..=[`REFRESH_MAX`]).
    pub refresh_secs: Option<u64>,
}

/// The `[keys]` table: optional remaps of a SAFE subset of action keys. Each
/// value is the single char the user wants to bind that action to. Frozen
/// globals can never be remapped, and invalid/duplicate remaps are dropped when
/// the [`crate::KeyMap`] is built (see [`crate::KeyMap::from_config`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyConfig {
    pub sync: Option<String>,
    pub goto: Option<String>,
    pub run: Option<String>,
    pub shell: Option<String>,
    pub files: Option<String>,
    pub problems: Option<String>,
    pub watch: Option<String>,
    pub fetch: Option<String>,
}

/// Lower bound for the idle auto-refresh cadence (seconds).
pub const REFRESH_MIN: u64 = 2;
/// Upper bound for the idle auto-refresh cadence (seconds).
pub const REFRESH_MAX: u64 = 60;
/// The cadence used when the config sets nothing (matches the historic default).
pub const REFRESH_DEFAULT: u64 = 5;

impl Config {
    /// The config file path. `HAW_CONFIG` overrides it outright (used by tests
    /// and power users); otherwise it's `<config_dir>/config.toml`. `None` when
    /// no config directory can be determined (rare — headless environments).
    pub fn path() -> Option<PathBuf> {
        if let Some(over) = std::env::var_os("HAW_CONFIG").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(over));
        }
        directories::ProjectDirs::from("dev", "hawser", "haw")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Load the user config, tolerating a missing file (→ defaults). A malformed
    /// file returns an `Err` carrying a human-readable message the caller can
    /// surface as a TUI message; it never panics.
    pub fn load() -> Result<Config, String> {
        let Some(path) = Self::path() else {
            return Ok(Config::default());
        };
        Self::load_from(&path)
    }

    /// Load from an explicit path (testable seam). Missing file → defaults.
    pub fn load_from(path: &std::path::Path) -> Result<Config, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default());
            }
            Err(err) => return Err(format!("reading {}: {err}", path.display())),
        };
        Self::parse(&text)
    }

    /// Parse config TOML, tolerating unknown keys and partial tables.
    pub fn parse(text: &str) -> Result<Config, String> {
        toml::from_str(text).map_err(|err| format!("config parse error: {err}"))
    }

    /// Persist this config to the standard path, creating the directory if
    /// needed. Errors are returned (never panicked) so the caller can show a TUI
    /// message while keeping the live change.
    pub fn save(&self) -> Result<(), String> {
        let Some(path) = Self::path() else {
            return Err("no config directory available".to_string());
        };
        self.save_to(&path)
    }

    /// Persist to an explicit path (testable seam).
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("creating {}: {err}", parent.display()))?;
        }
        let text =
            toml::to_string_pretty(self).map_err(|err| format!("serializing config: {err}"))?;
        std::fs::write(path, text).map_err(|err| format!("writing {}: {err}", path.display()))
    }

    /// The clamped idle auto-refresh cadence in seconds.
    pub fn refresh_secs(&self) -> u64 {
        self.ui
            .refresh_secs
            .unwrap_or(REFRESH_DEFAULT)
            .clamp(REFRESH_MIN, REFRESH_MAX)
    }
}

/// Resolve the startup theme from env + config. Pure: `no_color` is whether a
/// non-empty `NO_COLOR` is present, `env_theme` is the (already-read) `HAW_THEME`
/// value, `config_theme` is `[ui].theme`. See the module-level precedence docs.
pub fn resolve_theme(no_color: bool, env_theme: Option<&str>, config_theme: Option<&str>) -> Theme {
    if no_color {
        return Theme::monochrome();
    }
    if let Some(t) = env_theme.and_then(Theme::by_name) {
        return t;
    }
    if let Some(t) = config_theme.and_then(Theme::by_name) {
        return t;
    }
    Theme::catppuccin()
}

/// Resolve the interactive editor from env + config + PATH probe. Pure: `config`
/// is `[ui].editor`, `visual`/`editor` are the (already-read) `$VISUAL`/`$EDITOR`
/// values, `candidates` is the PATH fallback list. Returns `None` only when
/// nothing is set and no candidate resolves (the caller adds a `vi` last resort).
pub fn resolve_editor_with(
    config: Option<&str>,
    visual: Option<String>,
    editor: Option<String>,
    candidates: &[&str],
    on_path: impl Fn(&str) -> bool,
) -> Option<String> {
    if let Some(v) = visual.filter(|s| !s.trim().is_empty()) {
        return Some(v);
    }
    if let Some(e) = editor.filter(|s| !s.trim().is_empty()) {
        return Some(e);
    }
    if let Some(c) = config.filter(|s| !s.trim().is_empty()) {
        return Some(c.to_string());
    }
    candidates
        .iter()
        .find(|c| on_path(c))
        .map(|c| (*c).to_string())
}
