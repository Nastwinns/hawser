//! Minimal argument parsing for the git-gate binary.

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
    pub phase: Option<String>,
    /// A specific repo path to scan via `--repo`; defaults to every repo.
    pub repo: Option<String>,
}

/// The help text for `--help`.
pub const HELP: &str = "\
haw-git-gate — a pre-commit-style hygiene/secret gate

USAGE:
    haw-git-gate [OPTIONS]

OPTIONS:
    --haw-phase <name>   The lifecycle phase this run is invoked for.
    --repo <path>        Scan only this repo path (default: every repo in context).
    --help               Print this help and exit.

Reads a haw.plugin/1 context from HAW_JSON (or stdin) and prints a
haw.plugin.report/1 report to stdout. Orchestrates gitleaks when present and
falls back to a clearly-labelled heuristic scan otherwise.
";

/// Parse the given arguments into a [`ParseOutcome`].
pub fn parse(args: &[String]) -> Result<ParseOutcome, String> {
    let mut phase = None;
    let mut repo = None;
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
            other => {
                if let Some(rest) = other.strip_prefix("--haw-phase=") {
                    phase = Some(rest.to_string());
                } else if let Some(rest) = other.strip_prefix("--repo=") {
                    repo = Some(rest.to_string());
                } else {
                    return Err(format!("unknown argument: {other}"));
                }
            }
        }
    }

    Ok(ParseOutcome::Run(Options { phase, repo }))
}
