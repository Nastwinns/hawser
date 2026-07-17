//! Minimal argument parsing for the `haw-jira` binary.

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
    /// An explicit issue key (`haw jira PROJ-123 …`).
    pub issue: Option<String>,
    /// Target transition status (`--to <name>`); defaults per phase.
    pub to: Option<String>,
    /// The phase name passed via `--haw-phase`, if any (hook mode).
    pub phase: Option<String>,
    /// Emit the machine action document instead of human output.
    pub json: bool,
    /// Force a dry-run even when Jira credentials are present (`--dry-run`).
    pub dry_run: bool,
}

/// The help text for `--help`.
pub const HELP: &str = "\
haw-jira — link a haw changeset to a Jira issue and transition it as the change lands

USAGE:
    haw-jira [ISSUE-KEY] [OPTIONS]

ARGS:
    ISSUE-KEY            Jira issue key, e.g. PROJ-123. Derived from the current
                        change/<ID> branch when omitted.

OPTIONS:
    --to <name>          Target transition status (e.g. \"In Review\", \"Done\").
    --format json        Emit the planned/performed action as JSON on stdout.
    --dry-run            Force a dry-run: print the comment/transition that
                         WOULD be performed and exit 0, even when creds exist.
    --haw-phase <name>   Lifecycle phase; emits a haw.plugin.report/1 report.
    -h, --help           Print this help and exit.

Reads a haw.plugin/1 context from HAW_JSON (or stdin). Config comes from the
environment: JIRA_URL, JIRA_USER, JIRA_TOKEN. When any is missing it runs a
dry-run: it prints the exact comment/transition it WOULD perform and exits 0.
With all three present it comments on the issue and transitions it via the Jira
REST API.
";

/// Parse the given arguments into a [`ParseOutcome`].
pub fn parse(args: &[String]) -> Result<ParseOutcome, String> {
    let mut opts = Options::default();
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(ParseOutcome::Help(HELP.to_string())),
            "--dry-run" => opts.dry_run = true,
            "--format" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--format requires a value (json)".to_string())?;
                if value != "json" {
                    return Err(format!("unsupported --format: {value} (only 'json')"));
                }
                opts.json = true;
            }
            "--to" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--to requires a value".to_string())?;
                opts.to = Some(value.clone());
            }
            "--haw-phase" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--haw-phase requires a value".to_string())?;
                opts.phase = Some(value.clone());
            }
            other => {
                if let Some(rest) = other.strip_prefix("--format=") {
                    if rest != "json" {
                        return Err(format!("unsupported --format: {rest} (only 'json')"));
                    }
                    opts.json = true;
                } else if let Some(rest) = other.strip_prefix("--to=") {
                    opts.to = Some(rest.to_string());
                } else if let Some(rest) = other.strip_prefix("--haw-phase=") {
                    opts.phase = Some(rest.to_string());
                } else if other.starts_with('-') {
                    return Err(format!("unknown argument: {other}"));
                } else if opts.issue.is_none() {
                    opts.issue = Some(other.to_string());
                } else {
                    return Err(format!("unexpected extra argument: {other}"));
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
    fn help_flags() {
        for f in ["--help", "-h"] {
            assert!(matches!(
                parse(&[f.to_string()]).unwrap(),
                ParseOutcome::Help(_)
            ));
        }
    }

    #[test]
    fn positional_issue_key() {
        assert_eq!(run(&["PROJ-9"]).issue.as_deref(), Some("PROJ-9"));
    }

    #[test]
    fn flags_parse() {
        let o = run(&[
            "PROJ-9",
            "--to",
            "Done",
            "--format",
            "json",
            "--haw-phase",
            "post-land",
        ]);
        assert_eq!(o.issue.as_deref(), Some("PROJ-9"));
        assert_eq!(o.to.as_deref(), Some("Done"));
        assert!(o.json);
        assert_eq!(o.phase.as_deref(), Some("post-land"));
    }

    #[test]
    fn dry_run_flag_parses() {
        let o = run(&["SCRUM-1", "--to", "En cours", "--dry-run"]);
        assert!(o.dry_run);
        assert_eq!(o.issue.as_deref(), Some("SCRUM-1"));
        assert_eq!(o.to.as_deref(), Some("En cours"));
    }

    #[test]
    fn dry_run_defaults_false() {
        assert!(!run(&["SCRUM-1"]).dry_run);
    }

    #[test]
    fn rejects_bad_format() {
        assert!(parse(&["--format".to_string(), "xml".to_string()]).is_err());
    }

    #[test]
    fn rejects_two_positionals() {
        assert!(parse(&["A-1".to_string(), "B-2".to_string()]).is_err());
    }

    #[test]
    fn rejects_unknown_flag() {
        assert!(parse(&["--nope".to_string()]).is_err());
    }
}
