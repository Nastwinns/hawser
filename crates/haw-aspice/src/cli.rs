//! Minimal argument parsing for the `haw-aspice` binary.

/// The outcome of parsing command-line arguments.
pub enum ParseOutcome {
    /// Print the given help text and exit successfully.
    Help(String),
    /// Run the plugin with the given options.
    Run(Options),
}

/// The parsed run options.
#[derive(Debug, Default, PartialEq)]
pub struct Options {
    /// The phase name passed via `--haw-phase`, if any (hook mode).
    pub phase: Option<String>,
    /// Emit the machine `haw.aspice/1` document instead of human output.
    pub json: bool,
    /// The ISO-8601 timestamp to stamp into the trace, if provided.
    pub at: Option<String>,
    /// The output directory override; defaults to the workspace root or cwd.
    pub out_dir: Option<String>,
}

/// The help text for `--help`.
pub const HELP: &str = "\
haw-aspice — generate ASPICE/qualification traceability from the pinned fleet

USAGE:
    haw-aspice [OPTIONS]

OPTIONS:
    --format json        Emit the machine haw.aspice/1 document on stdout.
    --at <iso>           Timestamp to stamp into the trace (ISO-8601). Falls
                         back to SOURCE_DATE_EPOCH, else omitted.
    --out-dir <path>     Write artifacts here (default: workspace root, or cwd).
    --haw-phase <name>   Lifecycle phase; emits a haw.plugin.report/1 report.
    -h, --help           Print this help and exit.

Reads a haw.plugin/1 context from HAW_JSON (or stdin) and produces an
Automotive SPICE-style traceability bundle for the pinned fleet: aspice-trace.json
(machine, schema haw.aspice/1) and aspice-trace.md (human, repo -> pinned SHA ->
group mapped to ASPICE process areas). Enriches from `haw status --format json`
when haw is on PATH.
";

/// Parse the given arguments into a [`ParseOutcome`].
pub fn parse(args: &[String]) -> Result<ParseOutcome, String> {
    let mut opts = Options::default();
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(ParseOutcome::Help(HELP.to_string())),
            "--format" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--format requires a value (json)".to_string())?;
                if value != "json" {
                    return Err(format!("unsupported --format: {value} (only 'json')"));
                }
                opts.json = true;
            }
            "--haw-phase" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--haw-phase requires a value".to_string())?;
                opts.phase = Some(value.clone());
            }
            "--at" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--at requires a value".to_string())?;
                opts.at = Some(value.clone());
            }
            "--out-dir" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--out-dir requires a value".to_string())?;
                opts.out_dir = Some(value.clone());
            }
            other => {
                if let Some(rest) = other.strip_prefix("--format=") {
                    if rest != "json" {
                        return Err(format!("unsupported --format: {rest} (only 'json')"));
                    }
                    opts.json = true;
                } else if let Some(rest) = other.strip_prefix("--haw-phase=") {
                    opts.phase = Some(rest.to_string());
                } else if let Some(rest) = other.strip_prefix("--at=") {
                    opts.at = Some(rest.to_string());
                } else if let Some(rest) = other.strip_prefix("--out-dir=") {
                    opts.out_dir = Some(rest.to_string());
                } else {
                    return Err(format!("unknown argument: {other}"));
                }
            }
        }
    }

    Ok(ParseOutcome::Run(opts))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Options {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse(&owned).unwrap() {
            ParseOutcome::Run(o) => o,
            ParseOutcome::Help(_) => panic!("expected Run"),
        }
    }

    #[test]
    fn defaults_are_empty() {
        assert_eq!(run(&[]), Options::default());
    }

    #[test]
    fn help_flags_are_recognized() {
        for flag in ["--help", "-h"] {
            assert!(matches!(
                parse(&[flag.to_string()]).unwrap(),
                ParseOutcome::Help(_)
            ));
        }
    }

    #[test]
    fn parses_format_json() {
        assert!(run(&["--format", "json"]).json);
        assert!(run(&["--format=json"]).json);
    }

    #[test]
    fn parses_phase_and_at_and_outdir() {
        let o = run(&[
            "--haw-phase",
            "post-land",
            "--at",
            "2026-07-16T00:00:00Z",
            "--out-dir",
            "/tmp/x",
        ]);
        assert_eq!(o.phase.as_deref(), Some("post-land"));
        assert_eq!(o.at.as_deref(), Some("2026-07-16T00:00:00Z"));
        assert_eq!(o.out_dir.as_deref(), Some("/tmp/x"));
    }

    #[test]
    fn rejects_bad_format() {
        assert!(parse(&["--format".to_string(), "yaml".to_string()]).is_err());
    }

    #[test]
    fn rejects_unknown_argument() {
        assert!(parse(&["--nope".to_string()]).is_err());
    }
}
