//! Command-line argument parsing for `haw-compliance`.

use std::path::PathBuf;

/// Program name used in the help text and diagnostics.
const PROG: &str = "haw-compliance";

/// Parsed, validated command-line options for a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The optional phase name passed via `--haw-phase`.
    pub phase: Option<String>,
    /// The optional explicit output directory passed via `--out`.
    pub out: Option<PathBuf>,
}

impl Options {
    /// Resolve the output directory for SBOM files.
    ///
    /// If `--out` was given it is used verbatim. Otherwise the default is
    /// `.haw/sbom` under `root` (the workspace root from the context) when
    /// available, or under the current working directory otherwise.
    pub fn resolve_out_dir(&self, root: Option<&str>) -> PathBuf {
        if let Some(out) = &self.out {
            return out.clone();
        }
        let base = match root {
            Some(r) => PathBuf::from(r),
            None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        base.join(".haw").join("sbom")
    }
}

/// The outcome of parsing arguments: either a help request or options to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    /// `--help`/`-h` was requested; carries the help text to print.
    Help(String),
    /// A normal run with the given options.
    Run(Options),
}

/// Parse the given argument list (excluding the program name).
pub fn parse(args: &[String]) -> Result<ParseOutcome, String> {
    let mut phase: Option<String> = None;
    let mut out: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--help" | "-h" => return Ok(ParseOutcome::Help(help_text())),
            "--haw-phase" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "--haw-phase requires a value".to_string())?;
                phase = Some(value.clone());
                i += 2;
            }
            "--out" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "--out requires a value".to_string())?;
                out = Some(PathBuf::from(value));
                i += 2;
            }
            other if other.starts_with("--haw-phase=") => {
                phase = Some(other["--haw-phase=".len()..].to_string());
                i += 1;
            }
            other if other.starts_with("--out=") => {
                out = Some(PathBuf::from(&other["--out=".len()..]));
                i += 1;
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }

    Ok(ParseOutcome::Run(Options { phase, out }))
}

/// The `--help` text.
fn help_text() -> String {
    format!(
        "{PROG} — generate a composition-level SBOM (CycloneDX 1.5 + SPDX 2.3)\n\
         \n\
         USAGE:\n    \
         {PROG} [OPTIONS]\n\
         \n\
         Reads a haw.plugin/1 context from the HAW_JSON env var (fallback: stdin)\n\
         and writes sbom.cdx.json and sbom.spdx.json to the output directory.\n\
         \n\
         OPTIONS:\n    \
         --haw-phase <name>    Phase name to echo back in the report\n    \
         --out <dir>           Output directory for SBOM files\n                          \
         (default: <root>/.haw/sbom, or ./.haw/sbom)\n    \
         -h, --help            Print this help\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty() {
        let out = parse(&[]).unwrap();
        assert_eq!(
            out,
            ParseOutcome::Run(Options {
                phase: None,
                out: None
            })
        );
    }

    #[test]
    fn parses_phase_and_out_spaced() {
        let args = vec![
            "--haw-phase".to_string(),
            "verify".to_string(),
            "--out".to_string(),
            "/tmp/x".to_string(),
        ];
        let out = parse(&args).unwrap();
        assert_eq!(
            out,
            ParseOutcome::Run(Options {
                phase: Some("verify".to_string()),
                out: Some(PathBuf::from("/tmp/x")),
            })
        );
    }

    #[test]
    fn parses_equals_form() {
        let args = vec!["--haw-phase=build".to_string(), "--out=/tmp/y".to_string()];
        let out = parse(&args).unwrap();
        assert_eq!(
            out,
            ParseOutcome::Run(Options {
                phase: Some("build".to_string()),
                out: Some(PathBuf::from("/tmp/y")),
            })
        );
    }

    #[test]
    fn help_flag() {
        match parse(&["--help".to_string()]).unwrap() {
            ParseOutcome::Help(text) => assert!(text.contains("USAGE")),
            other => panic!("expected help, got {other:?}"),
        }
    }

    #[test]
    fn missing_value_errors() {
        assert!(parse(&["--out".to_string()]).is_err());
        assert!(parse(&["--haw-phase".to_string()]).is_err());
    }

    #[test]
    fn unknown_arg_errors() {
        assert!(parse(&["--nope".to_string()]).is_err());
    }

    #[test]
    fn resolve_out_dir_prefers_explicit() {
        let opts = Options {
            phase: None,
            out: Some(PathBuf::from("/tmp/explicit")),
        };
        assert_eq!(
            opts.resolve_out_dir(Some("/root")),
            PathBuf::from("/tmp/explicit")
        );
    }

    #[test]
    fn resolve_out_dir_uses_root() {
        let opts = Options {
            phase: None,
            out: None,
        };
        assert_eq!(
            opts.resolve_out_dir(Some("/root")),
            PathBuf::from("/root").join(".haw").join("sbom")
        );
    }
}
