use crate::cli::{Cli, Command};
use crate::diagnostics::Severity;
use crate::discover::{DiscoveredFile, DiscoveryError};
use crate::file_io::CommitError;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Overall result of a passdown run, ordered from least to most severe.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RunStatus {
    /// Every processed file already conforms and no operation failed.
    #[default]
    Clean,
    /// Formatting differences or unfixable document diagnostics were found.
    Issues,
    /// An operational failure occurred.
    Failure,
}

impl RunStatus {
    /// Combine independent outcomes, retaining the more severe status.
    pub const fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Failure, _) | (_, Self::Failure) => Self::Failure,
            (Self::Issues, _) | (_, Self::Issues) => Self::Issues,
            (Self::Clean, Self::Clean) => Self::Clean,
        }
    }
}

impl From<RunStatus> for ExitCode {
    fn from(status: RunStatus) -> Self {
        Self::from(match status {
            RunStatus::Clean => 0,
            RunStatus::Issues => 1,
            RunStatus::Failure => 2,
        })
    }
}

/// Run passdown, render its report, and return the highest-severity outcome.
/// A failure on one file does not stop the remaining files.
pub(crate) fn run(cli: Cli) -> RunStatus {
    let report = execute(cli);
    report.render();
    report.status
}

fn execute(cli: Cli) -> RunReport {
    let (mode, paths) = match cli.command {
        Command::Fix { paths } => (Mode::Fix, paths),
        Command::Check { paths } => (Mode::Check, paths),
    };

    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(err) => {
            return RunReport::failure(format!("passdown: failed to get current directory: {err}"));
        }
    };
    let policy = match crate::config::load(&cwd) {
        Ok(policy) => policy,
        Err(err) => {
            return RunReport::failure(format!("passdown: {err:#}"));
        }
    };

    let discovered = crate::discover::discover(&paths, &policy);

    // Files are processed in parallel but reported in sorted order.
    let outcomes: Vec<FileOutcome> = discovered
        .files
        .par_iter()
        .map(|file| process_file(mode, file))
        .collect();
    RunReport::from_outcomes(discovered.errors, outcomes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Fix,
    Check,
}

/// One file's result with its messages buffered per stream, so files can be
/// processed in parallel and still printed in deterministic sorted order.
#[derive(Debug)]
struct FileOutcome {
    display_path: PathBuf,
    status: RunStatus,
    stdout: Vec<String>,
    stderr: Vec<String>,
}

/// One reportable unit of a run, ordered for output by `cmp_for_report`.
#[derive(Debug)]
enum ReportEntry {
    SetupFailure(String),
    Discovery(DiscoveryError),
    File(FileOutcome),
}

impl ReportEntry {
    fn path(&self) -> Option<&Path> {
        match self {
            Self::SetupFailure(_) => None,
            Self::Discovery(error) => Some(error.path()),
            Self::File(outcome) => Some(&outcome.display_path),
        }
    }

    fn status(&self) -> RunStatus {
        match self {
            Self::SetupFailure(_) => RunStatus::Failure,
            Self::Discovery(error) => error.status(),
            Self::File(outcome) => outcome.status,
        }
    }

    /// Tie-break within one path: setup, then discovery, then file output.
    fn kind_rank(&self) -> u8 {
        match self {
            Self::SetupFailure(_) => 0,
            Self::Discovery(_) => 1,
            Self::File(_) => 2,
        }
    }

    /// Deterministic report order: path first (pathless setup failures sort
    /// ahead of everything), then entry kind, then message content.
    fn cmp_for_report(&self, other: &Self) -> Ordering {
        self.path()
            .cmp(&other.path())
            .then_with(|| self.kind_rank().cmp(&other.kind_rank()))
            .then_with(|| match (self, other) {
                (Self::SetupFailure(left), Self::SetupFailure(right)) => left.cmp(right),
                (Self::Discovery(left), Self::Discovery(right)) => left.cmp(right),
                _ => Ordering::Equal,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputStream {
    Stdout,
    Stderr,
}

struct RunReport {
    entries: Vec<ReportEntry>,
    status: RunStatus,
}

impl RunReport {
    fn failure(message: String) -> Self {
        Self::from_entries(vec![ReportEntry::SetupFailure(message)])
    }

    fn from_outcomes(errors: Vec<DiscoveryError>, outcomes: Vec<FileOutcome>) -> Self {
        let entries = errors
            .into_iter()
            .map(ReportEntry::Discovery)
            .chain(outcomes.into_iter().map(ReportEntry::File))
            .collect();
        Self::from_entries(entries)
    }

    fn from_entries(mut entries: Vec<ReportEntry>) -> Self {
        entries.sort_by(ReportEntry::cmp_for_report);
        let status = entries.iter().fold(RunStatus::Clean, |status, entry| {
            status.combine(entry.status())
        });
        Self { entries, status }
    }

    fn for_each_message(&self, mut visit: impl FnMut(OutputStream, &str)) {
        for entry in &self.entries {
            match entry {
                ReportEntry::SetupFailure(message) => {
                    visit(OutputStream::Stderr, message);
                }
                ReportEntry::Discovery(error) => {
                    // Discovery owns structured context; rendering remains at
                    // the application boundary with every other CLI message.
                    let message = render_discovery_error(error);
                    visit(OutputStream::Stderr, &message);
                }
                ReportEntry::File(outcome) => {
                    for message in &outcome.stdout {
                        visit(OutputStream::Stdout, message);
                    }
                    for message in &outcome.stderr {
                        visit(OutputStream::Stderr, message);
                    }
                }
            }
        }
    }

    fn render(&self) {
        self.for_each_message(|stream, message| match stream {
            OutputStream::Stdout => println!("{message}"),
            OutputStream::Stderr => eprintln!("{message}"),
        });
    }
}

fn render_discovery_error(error: &DiscoveryError) -> String {
    match error {
        DiscoveryError::MissingPath { path } => {
            format!("passdown: no such file or directory: {}", path.display())
        }
        DiscoveryError::Walk { message, .. } => format!("passdown: {message}"),
        DiscoveryError::Inspect { path, message } => {
            format!("passdown: failed to inspect {}: {message}", path.display())
        }
    }
}

/// Format one file and buffer its report. `check` prints what would change
/// (Note diagnostics included); `fix` rewrites the target atomically and
/// reports each rewritten file. Error diagnostics are reported in both modes.
fn process_file(mode: Mode, file: &DiscoveredFile) -> FileOutcome {
    let mut outcome = FileOutcome {
        display_path: file.display_path.clone(),
        status: RunStatus::Clean,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let path = &file.target_path;
    let display_path = &file.display_path;

    let input = match std::fs::read_to_string(path) {
        Ok(input) => input,
        Err(err) => {
            outcome.stderr.push(format!(
                "passdown: failed to read {}: {err}",
                display_path.display()
            ));
            outcome.status = RunStatus::Failure;
            return outcome;
        }
    };

    let result = crate::format::format_document(&input);

    for diag in result
        .diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
    {
        outcome.stderr.push(diag.render(display_path));
        outcome.status = RunStatus::Issues;
    }

    let changed = result.output != input;
    match mode {
        Mode::Check => {
            if changed {
                outcome
                    .stdout
                    .push(format!("would reformat: {}", display_path.display()));
                for diag in result
                    .diagnostics
                    .iter()
                    .filter(|d| d.severity() == Severity::Note)
                {
                    outcome.stdout.push(diag.render(display_path));
                }
                outcome.status = outcome.status.combine(RunStatus::Issues);
            }
        }
        Mode::Fix => {
            if changed {
                match crate::file_io::atomic_replace(path, file.identity, result.output.as_bytes())
                {
                    Ok(()) => {
                        outcome
                            .stdout
                            .push(format!("fixed: {}", display_path.display()));
                    }
                    Err(CommitError::HardLinked(count)) => {
                        outcome.stderr.push(format!(
                            "passdown: refusing to rewrite hard-linked file {}: target has {count} links",
                            display_path.display()
                        ));
                        outcome.status = RunStatus::Failure;
                    }
                    Err(err) => {
                        outcome.stderr.push(format!(
                            "passdown: failed to write {}: {err}",
                            display_path.display()
                        ));
                        outcome.status = RunStatus::Failure;
                    }
                }
            }
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_outcome(
        path: &str,
        status: RunStatus,
        stdout: &[&str],
        stderr: &[&str],
    ) -> FileOutcome {
        FileOutcome {
            display_path: PathBuf::from(path),
            status,
            stdout: stdout.iter().map(ToString::to_string).collect(),
            stderr: stderr.iter().map(ToString::to_string).collect(),
        }
    }

    fn messages(report: &RunReport) -> Vec<(OutputStream, String)> {
        let mut messages = Vec::new();
        report.for_each_message(|stream, message| {
            messages.push((stream, message.to_owned()));
        });
        messages
    }

    #[test]
    fn report_merges_entries_in_deterministic_path_order() {
        let report = RunReport::from_outcomes(
            vec![
                DiscoveryError::MissingPath {
                    path: PathBuf::from("z.md"),
                },
                DiscoveryError::MissingPath {
                    path: PathBuf::from("a.md"),
                },
            ],
            vec![file_outcome(
                "m.md",
                RunStatus::Issues,
                &["m stdout"],
                &["m stderr"],
            )],
        );

        let paths: Vec<_> = report
            .entries
            .iter()
            .map(|entry| entry.path().unwrap())
            .collect();
        assert_eq!(
            paths,
            [Path::new("a.md"), Path::new("m.md"), Path::new("z.md")]
        );
        assert_eq!(report.status, RunStatus::Failure);
    }

    #[test]
    fn discovery_precedes_file_output_for_the_same_path() {
        let report = RunReport::from_outcomes(
            vec![DiscoveryError::Inspect {
                path: PathBuf::from("doc.md"),
                message: "inspection failed".to_owned(),
            }],
            vec![file_outcome(
                "doc.md",
                RunStatus::Issues,
                &["file stdout"],
                &["file stderr"],
            )],
        );

        let messages = messages(&report);
        assert_eq!(
            messages,
            [
                (
                    OutputStream::Stderr,
                    "passdown: failed to inspect doc.md: inspection failed".to_owned()
                ),
                (OutputStream::Stdout, "file stdout".to_owned()),
                (OutputStream::Stderr, "file stderr".to_owned()),
            ]
        );
        assert_eq!(report.status, RunStatus::Failure);
    }

    #[test]
    fn report_uses_highest_file_status_and_preserves_message_streams() {
        let report = RunReport::from_outcomes(
            Vec::new(),
            vec![
                file_outcome(
                    "a.md",
                    RunStatus::Issues,
                    &["first", "second"],
                    &["problem"],
                ),
                file_outcome("b.md", RunStatus::Clean, &[], &[]),
            ],
        );

        assert_eq!(
            messages(&report),
            [
                (OutputStream::Stdout, "first".to_owned()),
                (OutputStream::Stdout, "second".to_owned()),
                (OutputStream::Stderr, "problem".to_owned()),
            ]
        );
        assert_eq!(report.status, RunStatus::Issues);
    }

    #[test]
    fn status_combination_retains_the_highest_severity() {
        assert_eq!(
            RunStatus::Clean.combine(RunStatus::Issues),
            RunStatus::Issues
        );
        assert_eq!(
            RunStatus::Issues.combine(RunStatus::Clean),
            RunStatus::Issues
        );
        assert_eq!(
            RunStatus::Issues.combine(RunStatus::Failure),
            RunStatus::Failure
        );
        assert_eq!(
            RunStatus::Failure.combine(RunStatus::Clean),
            RunStatus::Failure
        );
    }
}
