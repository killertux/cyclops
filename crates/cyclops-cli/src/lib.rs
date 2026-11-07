//! The composition root, and the `cyclops` command line. Every interface and the
//! specification get their services from here, so that they all run the same analysis.

mod environment;
mod output;

pub use environment::Environment;

use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::analyzers::{CohesionAnalyzer, ComplexityAnalyzer, SizeAnalyzer};
use cyclops_application::diagram::{DiagramRequest, Level};
use cyclops_application::graph::CouplingAnalyzer;
use cyclops_application::pipeline::{AnalysisService, Sources};
use cyclops_application::ports::{CacheAccess, CacheError, HistorySource, LanguageAdapter};
use cyclops_application::precise::Precise;
use cyclops_application::queries::Order;
use cyclops_domain::filter::Filter;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::settings::{
    Layer, Origin, RawValue, Settings, SettingsError, Severity, initial_config,
};
use cyclops_domain::symbol::SymbolId;
use cyclops_infra::{
    Export, ExportFormat, GitHistory, ProjectFiles, ProjectFolder, SqliteFactStore, UserConfig,
    read_layer,
};
use cyclops_lang_rust::RustAdapter;
use cyclops_lang_typescript::TypeScriptAdapter;
use cyclops_semantic::{Tools, providers};
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
    analyze_with(root, environment, flags, adapters(), false)
}

/// Analyzes the project in a folder, with these languages. `precise` asks for the
/// indexers as well.
///
/// # Errors
///
/// Fails if the folder is not a project, its settings are wrong, or it cannot be listed.
pub fn analyze_with(
    root: &Path,
    environment: &Environment,
    flags: &Layer,
    adapters: Vec<Box<dyn LanguageAdapter>>,
    precise: bool,
) -> Result<ProjectAnalysis> {
    let files = ProjectFiles::open(root)?;
    let settings = settings(root, environment, flags, true)?;
    run_analysis(root, environment, &files, &settings, adapters, precise)
}

/// Analyzes with settings already worked out, using the project's cache and history.
fn run_analysis(
    root: &Path,
    environment: &Environment,
    files: &ProjectFiles,
    settings: &Settings,
    adapters: Vec<Box<dyn LanguageAdapter>>,
    precise: bool,
) -> Result<ProjectAnalysis> {
    let folder = ProjectFolder::of(root);
    let tools = Tools::new(environment.get("PATH"));
    let cache_path = folder.cache_path();
    let indexers = providers(root, cache_path.parent().unwrap_or(root), &tools);
    let store = folder
        .prepare()
        .map_err(|error| CacheError(error.to_string()))
        .and_then(|()| SqliteFactStore::open(&folder.cache_path()));
    let service = analysis_service(adapters);
    let history = GitHistory::open(root);
    let history = history
        .as_ref()
        .map(|history| history as &dyn HistorySource);
    let mut store = store;
    let cache = match &mut store {
        Ok(store) => CacheAccess::Store(store),
        Err(error) => CacheAccess::Unavailable(CacheError(error.to_string())),
    };
    let sources = Sources {
        files,
        cache,
        history,
        precise: Precise {
            providers: &indexers,
            asked: precise,
        },
    };
    Ok(service.analyze(sources, settings)?)
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
    Analyze {
        /// Also run the compilers' indexers, to make guessed relations exact. It is slow,
        /// and may run the project's build scripts.
        #[arg(long)]
        precise: bool,
        #[command(flatten)]
        tests: TestFlags,
    },
    /// Print every metric.
    Report {
        /// `table` for people, or `json` for tools.
        #[arg(long, default_value = "table")]
        format: String,
        #[command(flatten)]
        tests: TestFlags,
    },
    /// Find symbols with a filter, one on each line.
    Query {
        /// The filter, such as 'kind = function and cyclomatic > 15'.
        filter: String,
        /// A metric and `asc` or `desc`, such as 'cyclomatic desc'.
        #[arg(long)]
        sort: Option<String>,
        /// Keep only the first results.
        #[arg(long)]
        limit: Option<usize>,
        /// `lines`, or `json` for tools.
        #[arg(long, default_value = "lines")]
        format: String,
        #[command(flatten)]
        tests: TestFlags,
    },
    /// Check the alert rules, and fail when an alert reaches a severity.
    Check {
        /// `warning` or `error`. It beats `check.fail_on`.
        #[arg(long)]
        fail_on: Option<String>,
        #[command(flatten)]
        tests: TestFlags,
    },
    /// Export a diagram for other tools.
    Export {
        /// `mermaid`, `dot` or `plantuml`.
        #[arg(long)]
        format: String,
        /// `package`, `module`, `type` or `member`.
        #[arg(long, default_value = "module")]
        level: String,
        /// The symbol to start from.
        #[arg(long)]
        focus: Option<String>,
        /// How many relations to follow from the focus.
        #[arg(long)]
        depth: Option<u32>,
        /// A metric to attach to each node.
        #[arg(long)]
        overlay: Option<String>,
        #[command(flatten)]
        tests: TestFlags,
    },
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
        Ok(code) => code,
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

fn execute(cli: &Cli, environment: &Environment) -> Result<ExitCode> {
    let root = cli.project.as_deref().unwrap_or_else(|| Path::new("."));
    let mut out = io::stdout().lock();
    match &cli.command {
        Command::Init => {
            writeln!(out, "{}", init(root)?)?;
        }
        Command::Analyze { precise, tests } => {
            let analysis = analyze_with(root, environment, &tests.layer(), adapters(), *precise)?;
            report_problems(&analysis);
            summary(&mut out, &analysis)?;
        }
        Command::Report { format, tests } => {
            let json = match format.as_str() {
                "table" => false,
                "json" => true,
                other => return Err(anyhow!("Unknown format: {other}")),
            };
            let analysis = analyze(root, environment, &tests.layer())?;
            report_problems(&analysis);
            if json {
                writeln!(out, "{}", output::report_json(&analysis))?;
            } else {
                metrics(&mut out, &analysis)?;
            }
        }
        Command::Query {
            filter,
            sort,
            limit,
            format,
            tests,
        } => {
            let json = match format.as_str() {
                "lines" => false,
                "json" => true,
                other => return Err(anyhow!("Unknown format: {other}")),
            };
            let filter = Filter::parse(filter)?;
            let sort = sort.as_deref().map(parse_sort).transpose()?;
            let analysis = analyze(root, environment, &tests.layer())?;
            let found = analysis.search(&filter, sort, *limit);
            if json {
                writeln!(out, "{}", output::symbols_json(&found))?;
            } else {
                for symbol in found {
                    writeln!(out, "{}", symbol.id)?;
                }
            }
        }
        Command::Check { fail_on, tests } => {
            return check(
                root,
                environment,
                &tests.layer(),
                fail_on.as_deref(),
                &mut out,
            );
        }
        Command::Export {
            format,
            level,
            focus,
            depth,
            overlay,
            tests,
        } => {
            let format =
                ExportFormat::parse(format).ok_or_else(|| anyhow!("Unknown format: {format}"))?;
            let level = Level::parse(level).ok_or_else(|| anyhow!("Unknown level: {level}"))?;
            let overlay = overlay
                .as_deref()
                .map(|name| MetricId::parse(name).ok_or_else(|| anyhow!("Unknown metric: {name}")))
                .transpose()?;
            let analysis = analyze(root, environment, &tests.layer())?;
            let request = DiagramRequest {
                level,
                focus: focus.as_deref().map(SymbolId::new),
                depth: depth.unwrap_or(u32::from(focus.is_some())),
                relations: Vec::new(),
                overlay,
                filter: None,
            };
            let view = analysis.diagram(&request);
            let written = Export {
                view: &view,
                level,
                overlay,
            }
            .write(format)?;
            write!(out, "{written}")?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn parse_sort(text: &str) -> Result<(MetricId, Order)> {
    let mut words = text.split_whitespace();
    let name = words.next().unwrap_or_default();
    let metric = MetricId::parse(name).ok_or_else(|| anyhow!("Unknown metric: {name}"))?;
    let order = match words.next() {
        None | Some("asc") => Order::Ascending,
        Some("desc") => Order::Descending,
        Some(other) => return Err(anyhow!("Unknown order: {other}. Use asc or desc.")),
    };
    Ok((metric, order))
}

/// Checks the alert rules, ignoring your own settings, so that the answer is the same
/// on every machine.
fn check(
    root: &Path,
    environment: &Environment,
    flags: &Layer,
    fail_on: Option<&str>,
    out: &mut impl Write,
) -> Result<ExitCode> {
    let fail_on = match fail_on {
        None => None,
        Some("warning") => Some(Severity::Warning),
        Some("error") => Some(Severity::Error),
        Some(other) => return Err(anyhow!("Unknown severity: {other}. Use warning or error.")),
    };
    let files = ProjectFiles::open(root)?;
    let settings = settings(root, environment, flags, false)?;
    let analysis = run_analysis(root, environment, &files, &settings, adapters(), false)?;
    let threshold = fail_on.unwrap_or(settings.fail_on);
    let mut failed = false;
    for alert in analysis.alerts() {
        let severity = if alert.severity == Severity::Error {
            "error"
        } else {
            "warning"
        };
        let value = alert.metric.descriptor().format(alert.value);
        writeln!(
            out,
            "{severity} {}  {}={value}",
            alert.subject, alert.metric
        )?;
        failed |= alert.severity >= threshold;
    }
    Ok(if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
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
