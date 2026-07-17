//! Minimal argument parsing for the misra binary.

/// The output format for the report.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable summary (the default for the `haw misra` subcommand).
    Human,
    /// The raw `haw.plugin.report/1` JSON document.
    Json,
}

/// The outcome of parsing command-line arguments.
pub enum ParseOutcome {
    /// Print the given help text and exit successfully.
    Help(String),
    /// Run the gate with the given options.
    Run(Options),
}

/// The parsed run options.
pub struct Options {
    /// The phase name passed via `--haw-phase`, if any.
    ///
    /// When a phase is present the plugin is being dispatched as a lifecycle
    /// hook and always emits JSON, regardless of `--format`.
    pub phase: Option<String>,
    /// A specific repo path to scan via `--repo`; defaults to every repo.
    pub repo: Option<String>,
    /// The requested output format (overridden to JSON in hook mode).
    pub format: Format,
}

/// The help text for `--help`.
pub const HELP: &str = "\
haw-misra — a MISRA C static-analysis gate across the fleet

USAGE:
    haw misra [OPTIONS]
    haw-misra [OPTIONS]

OPTIONS:
    --haw-phase <name>   The lifecycle phase this run is invoked for.
    --repo <path>        Scan only this repo path (default: every repo in context).
    --format <fmt>       Output format: `human` (default) or `json`.
    --help               Print this help and exit.

Runs a MISRA C pass over each repo's tracked C/C++ sources by shelling out to
cppcheck with `--addon=misra`. Reads a haw.plugin/1 context from HAW_JSON (or
stdin) and prints a haw.plugin.report/1 report.

As a subcommand (`haw misra`) it prints a human summary — files scanned and the
violation count. As a `pre-request` lifecycle hook it emits one report and
BLOCKS the PR (ok:false, one error-level finding per violation) when violations
exist.

FAIL-OPEN: if cppcheck is not on PATH, or a repo has no C/C++ files, the gate is
skipped with a warn-level finding and exit 0 — a missing tool never blocks
adoption. Set HAW_JSON to override the context source.
";

/// Parse the given arguments into a [`ParseOutcome`].
pub fn parse(args: &[String]) -> Result<ParseOutcome, String> {
    let mut phase = None;
    let mut repo = None;
    let mut format = Format::Human;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(ParseOutcome::Help(HELP.to_string())),
            "--haw-phase" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--haw-phase requires a value".to_string())?;
                phase = Some(value.clone());
            }
            "--repo" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--repo requires a value".to_string())?;
                repo = Some(value.clone());
            }
            "--format" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--format requires a value".to_string())?;
                format = parse_format(value)?;
            }
            other => {
                if let Some(rest) = other.strip_prefix("--haw-phase=") {
                    phase = Some(rest.to_string());
                } else if let Some(rest) = other.strip_prefix("--repo=") {
                    repo = Some(rest.to_string());
                } else if let Some(rest) = other.strip_prefix("--format=") {
                    format = parse_format(rest)?;
                } else {
                    return Err(format!("unknown argument: {other}"));
                }
            }
        }
    }

    Ok(ParseOutcome::Run(Options {
        phase,
        repo,
        format,
    }))
}

/// Parse a `--format` value into a [`Format`].
fn parse_format(value: &str) -> Result<Format, String> {
    match value {
        "human" | "text" => Ok(Format::Human),
        "json" => Ok(Format::Json),
        other => Err(format!("unknown format: {other} (want `human` or `json`)")),
    }
}
