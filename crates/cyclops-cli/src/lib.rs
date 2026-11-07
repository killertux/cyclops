//! The composition root, and the `cyclops` command line. Every interface and the
//! specification get their services from here, so that they all run the same analysis.

mod environment;

pub use environment::Environment;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::analyzers::{CohesionAnalyzer, ComplexityAnalyzer, SizeAnalyzer};
use cyclops_application::graph::CouplingAnalyzer;
use cyclops_application::pipeline::AnalysisService;
use cyclops_application::ports::{CacheAccess, CacheError, LanguageAdapter};
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::settings::{Layer, Origin, RawValue, Settings, SettingsError, initial_config};
use cyclops_infra::{ProjectFiles, ProjectFolder, SqliteFactStore, UserConfig, read_layer};
use cyclops_lang_rust::RustAdapter;
use cyclops_lang_typescript::TypeScriptAdapter;
use std::collections::BTreeMap;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// How messages name a project's own settings file.
const PROJECT_FILE: &str = ".cyclops/config.toml";

/// Every language Cyclops has.
#[must_use]
pub fn adapters() -> Vec<Box<dyn LanguageAdapter>> {
    vec![Box::new(RustAdapter), Box::new(TypeScriptAdapter)]
}

/// Builds the analysis with these languages and every metric Cyclops has.
#[must_use]
pub fn analysis_service(adapters: Vec<Box<dyn LanguageAdapter>>) -> AnalysisService {
    AnalysisService::new(
        adapters,
        vec![
            Box::new(ComplexityAnalyzer),
            Box::new(SizeAnalyzer),
            Box::new(CohesionAnalyzer),
            Box::new(CouplingAnalyzer),
        ],
    )
}

/// The settings a command line flag gives, such as `--tests`.
#[must_use]
pub fn flag_layer(tests: Option<bool>) -> Layer {
    let mut layer = Layer::new(Origin::Flag);
    if let Some(include) = tests {
        layer
            .values
            .push(("tests.include".to_owned(), RawValue::Bool(include)));
    }
    layer
}

/// Works out the settings of a project. `yours` says whether your own file counts, which
/// it does everywhere except `cyclops check`.
///
/// # Errors
///
/// Fails if a settings file cannot be read, or anything in it is wrong.
pub fn settings(
    root: &Path,
    environment: &Environment,
    flags: &Layer,
    yours: bool,
) -> Result<Settings, SettingsError> {
    let user = UserConfig::locate(environment.home(), environment.xdg_config_home());
    let label = user.as_ref().map_or_else(
        || "~/.config/cyclops/config.toml".to_owned(),
        |user| user.label.clone(),
    );
    let mut layers = Vec::new();
    if yours && let Some(user) = &user {
        layers.extend(read_layer(&user.path, Origin::UserFile, &user.label)?);
    }
    let project = ProjectFolder::of(root).config_path();
    layers.extend(read_layer(&project, Origin::ProjectFile, PROJECT_FILE)?);
    layers.push(Settings::environment_layer(|name| environment.get(name)));
    layers.push(flags.clone());
    Settings::resolve(&layers, &label)
}

/// Where your own settings file is, for this environment.
#[must_use]
pub fn user_config_path(environment: &Environment) -> Option<PathBuf> {
    UserConfig::locate(environment.home(), environment.xdg_config_home()).map(|user| user.path)
}

/// Analyzes the project in a folder, with every language.
///
/// # Errors
///
/// Fails if the folder is not a project, its settings are wrong, or it cannot be listed.
pub fn analyze(root: &Path, environment: &Environment, flags: &Layer) -> Result<ProjectAnalysis> {
    analyze_with(root, environment, flags, adapters())
}

/// Analyzes the project in a folder, with these languages.
///
/// # Errors
///
/// Fails if the folder is not a project, its settings are wrong, or it cannot be listed.
pub fn analyze_with(
    root: &Path,
    environment: &Environment,
    flags: &Layer,
    adapters: Vec<Box<dyn LanguageAdapter>>,
) -> Result<ProjectAnalysis> {
    let files = ProjectFiles::open(root)?;
    let settings = settings(root, environment, flags, true)?;
    let folder = ProjectFolder::of(root);
    let store = folder
        .prepare()
        .map_err(|error| CacheError(error.to_string()))
        .and_then(|()| SqliteFactStore::open(&folder.cache_path()));
    let service = analysis_service(adapters);
    let analysis = match store {
        Ok(mut store) => service.analyze(&files, CacheAccess::Store(&mut store), &settings)?,
        Err(error) => service.analyze(&files, CacheAccess::Unavailable(error), &settings)?,
    };
    Ok(analysis)
}

/// Creates `.cyclops/` with settings for the languages the project has. It never
/// overwrites existing settings. Returns what to tell the person.
///
/// # Errors
///
/// Fails if the folder is not a project, or the files cannot be written.
pub fn init(root: &Path) -> Result<String> {
    let files = ProjectFiles::open(root)?;
    let folder = ProjectFolder::of(root);
    if folder.has_config() {
        return Ok(format!("Already initialized: {PROJECT_FILE} exists."));
    }
    let languages = analysis_service(adapters()).languages_in(&files)?;
    folder.create_config(&initial_config(&languages))?;
    Ok(format!("Created {PROJECT_FILE}."))
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
    /// Create .cyclops/ with settings for this project.
    Init,
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
    fn layer(&self) -> Layer {
        let tests = match (self.tests, self.no_tests) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        };
        flag_layer(tests)
    }
}

/// Runs the command line, and returns what the process should exit with.
#[must_use]
pub fn run(arguments: impl IntoIterator<Item = String>, environment: &Environment) -> ExitCode {
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => return clap_outcome(&error),
    };
    match execute(&cli, environment) {
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

fn execute(cli: &Cli, environment: &Environment) -> Result<()> {
    let root = cli.project.as_deref().unwrap_or_else(|| Path::new("."));
    let mut out = io::stdout().lock();
    match &cli.command {
        Command::Init => {
            writeln!(out, "{}", init(root)?)?;
            Ok(())
        }
        Command::Analyze(flags) => {
            let analysis = analyze(root, environment, &flags.layer())?;
            report_problems(&analysis);
            summary(&mut out, &analysis)
        }
        Command::Report(flags) => {
            let analysis = analyze(root, environment, &flags.layer())?;
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
    let parsed = analysis.files_parsed();
    writeln!(out, "{files} files, {symbols} symbols, {parsed} parsed")?;
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
