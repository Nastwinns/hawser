//! Minimal ANSI painter for CLI output.

use std::io::IsTerminal;

/// Minimal ANSI painter: colored on a TTY, plain under `NO_COLOR` or when
/// piped; `CLICOLOR_FORCE=1` forces color even when piped (bat/eza convention).
/// Semantic helpers keep every command on one shared scheme:
/// cyan names, yellow revs, dim chrome, green/yellow/red state.
pub(crate) struct Palette {
    on: bool,
}

impl Palette {
    pub(crate) fn new() -> Self {
        let force = std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0");
        let on =
            std::env::var_os("NO_COLOR").is_none() && (force || std::io::stdout().is_terminal());
        Self { on }
    }

    pub(crate) fn paint(&self, code: &str, text: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    /// Repo/stack names: bold cyan.
    pub(crate) fn name(&self, text: &str) -> String {
        self.paint("1;36", text)
    }

    /// Revisions, tags, branches: yellow.
    pub(crate) fn rev(&self, text: &str) -> String {
        self.paint("33", text)
    }

    /// SHAs, paths, secondary chrome: dim.
    pub(crate) fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }

    /// Success marks and clean state: green.
    pub(crate) fn ok(&self, text: &str) -> String {
        self.paint("32", text)
    }

    /// Warnings (dirty): bold yellow.
    pub(crate) fn warn(&self, text: &str) -> String {
        self.paint("1;33", text)
    }

    /// Failures and drift: bold red.
    pub(crate) fn err(&self, text: &str) -> String {
        self.paint("1;31", text)
    }

    /// Table headers: bold + underline.
    pub(crate) fn header(&self, text: &str) -> String {
        self.paint("1;4", text)
    }

    /// Summary lines: bold.
    pub(crate) fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }
}
