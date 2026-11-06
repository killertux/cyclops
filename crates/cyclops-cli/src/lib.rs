//! The composition root, and the `cyclops` command line. Every interface and the
//! specification get their services from here, so that they all run the same analysis.

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::analyzers::{ComplexityAnalyzer, SizeAnalyzer};
use cyclops_application::pipeline::{AnalysisOptions, AnalysisService};
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::model::TestCode;
use cyclops_infra::ProjectFiles;
use cyclops_lang_rust::RustAdapter;
use std::collections::BTreeMap;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Builds the analysis with every language and metric Cyclops has.
#[must_use]
pub fn analysis_service() -> AnalysisService {
    AnalysisService::new(
        vec![Box::new(RustAdapter)],
        vec![Box::new(ComplexityAnalyzer), Box::new(SizeAnalyzer)],
    )
}

/// Analyzes the project in a folder.
///
/// # Errors
///
/// Fails if the folder is not a project, or cannot be listed.
pub fn analyze(root: &Path, options: AnalysisOptions) -> Result<ProjectAnalysis> {
    let files = ProjectFiles::open(root)?;
    Ok(analysis_service().analyze(&files, options)?)
}

#[derive(Debug, Parser)]
#[command(
    name = "cyclops",
    version,
    about = "Understand a codebase and judge its health"
)]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Work on this folder instead of the current one.
    #[arg(short = 'C', global = true, value_name = "PATH")]
    project: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Analyze the project and print a summary.
    Analyze(TestFlags),
    /// Print every metric.
    Report(TestFlags),
}

#[derive(Debug, Args)]
struct TestFlags {
    /// Include test code.
    #[arg(long, conflicts_with = "no_tests")]
    tests: bool,

    /// Leave test code out.
    #[arg(long)]
    no_tests: bool,
}

impl TestFlags {
    const fn options(&self) -> AnalysisOptions {
        let tests = if self.tests {
            TestCode::Included
        } else {
            TestCode::Excluded
        };
        AnalysisOptions { tests }
    }
}

/// Runs the command line, and returns what the process should exit with.
#[must_use]
pub fn run(arguments: impl IntoIterator<Item = String>) -> ExitCode {
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => return clap_outcome(&error),
    };
    match execute(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if is_closed_pipe(&error) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

/// Help and the version are answers, and anything else clap reports is a mistake.
fn clap_outcome(error: &clap::Error) -> ExitCode {
    let printed = error.print();
    if error.use_stderr() || printed.is_err() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

/// Whoever reads the output may stop early, as `head` does, and that is not a failure.
fn is_closed_pipe(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == ErrorKind::BrokenPipe)
}

fn execute(cli: &Cli) -> Result<()> {
    let root = cli.project.as_deref().unwrap_or_else(|| Path::new("."));
    let mut out = io::stdout().lock();
    match &cli.command {
        Command::Analyze(flags) => {
            let analysis = analyze(root, flags.options())?;
            report_problems(&analysis);
            summary(&mut out, &analysis)
        }
        Command::Report(flags) => {
            let analysis = analyze(root, flags.options())?;
            report_problems(&analysis);
            metrics(&mut out, &analysis)
        }
    }
}

fn report_problems(analysis: &ProjectAnalysis) {
    for problem in analysis.problems() {
        eprintln!("{problem}");
    }
}

fn summary(out: &mut impl Write, analysis: &ProjectAnalysis) -> Result<()> {
    let files = analysis.files().len();
    let symbols = analysis.view().symbols().count();
    writeln!(out, "{files} files, {symbols} symbols")?;
    Ok(())
}

fn metrics(out: &mut impl Write, analysis: &ProjectAnalysis) -> Result<()> {
    let mut rows: BTreeMap<&Subject, Vec<(MetricId, f64)>> = BTreeMap::new();
    for (subject, metric, value) in analysis.measurements() {
        rows.entry(subject).or_default().push((metric, value));
    }
    for (subject, values) in rows {
        let values: Vec<String> = values
            .into_iter()
            .map(|(metric, value)| format!("{metric}={}", metric.descriptor().format(value)))
            .collect();
        writeln!(out, "{subject}  {}", values.join(" "))?;
    }
    Ok(())
}
