//! The composition root, and the `cyclops` command line. Every interface and the
//! specification get their services from here, so that they all run the same analysis.

mod environment;
mod output;

pub use environment::Environment;

use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::analyzers::{CohesionAnalyzer, ComplexityAnalyzer, SizeAnalyzer};
use cyclops_application::diagram::{DiagramRequest, DiagramView, Level};
use cyclops_application::graph::CouplingAnalyzer;
use cyclops_application::pipeline::{AnalysisService, Sources};
use cyclops_application::ports::{
    CacheAccess, CacheError, HistorySource, IndexError, LanguageAdapter, Progress, Silent,
};
use cyclops_application::precise::Precise;
use cyclops_application::queries::Order;
use cyclops_domain::filter::Filter;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::problem::Problem;
use cyclops_domain::settings::{
    Layer, Origin, RawValue, SemanticMode, Settings, SettingsError, Severity, Theme, initial_config,
};
use cyclops_domain::symbol::SymbolId;
use cyclops_infra::{
    Export, ExportFormat, GitHistory, ProjectFiles, ProjectFolder, ProjectWatcher, SqliteFactStore,
    UserConfig, read_layer, save_alert, set_alert_enabled,
};
use cyclops_lang_rust::RustAdapter;
use cyclops_lang_typescript::TypeScriptAdapter;
use cyclops_semantic::{Tools, providers};
use cyclops_tui::state::{IndexerReport, Startup, ThemeChoice};
use cyclops_tui::{Backend, Core};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

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
    analyze_heard(root, environment, flags, adapters, precise, &Silent)
}

/// Analyzes the project in a folder, telling `progress` how it goes. `progress` may
/// stop it.
///
/// # Errors
///
/// Fails if the folder is not a project, its settings are wrong, it cannot be listed,
/// or the analysis was stopped.
pub fn analyze_heard(
    root: &Path,
    environment: &Environment,
    flags: &Layer,
    adapters: Vec<Box<dyn LanguageAdapter>>,
    precise: bool,
    progress: &dyn Progress,
) -> Result<ProjectAnalysis> {
    let files = ProjectFiles::open(root)?;
    let settings = settings(root, environment, flags, true)?;
    run_analysis(
        root,
        environment,
        &files,
        &settings,
        adapters,
        precise,
        progress,
    )
}

/// Analyzes with settings already worked out, using the project's cache and history.
fn run_analysis(
    root: &Path,
    environment: &Environment,
    files: &ProjectFiles,
    settings: &Settings,
    adapters: Vec<Box<dyn LanguageAdapter>>,
    precise: bool,
    progress: &dyn Progress,
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
        progress,
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
struct Cli {
    /// Work on this folder instead of the current one.
    #[arg(short = 'C', global = true, value_name = "PATH")]
    project: Option<PathBuf>,

    /// Without a command, the terminal interface opens.
    #[command(subcommand)]
    command: Option<Command>,
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
    let Some(command) = &cli.command else {
        return interactive(root, environment);
    };
    let mut out = io::stdout().lock();
    match command {
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

/// The interface's view of the core, for one project.
struct Interactive {
    root: PathBuf,
    environment: Environment,
}

impl Core for Interactive {
    fn analyze(
        &self,
        tests: Option<bool>,
        precise: bool,
        progress: &dyn Progress,
    ) -> Result<ProjectAnalysis, String> {
        analyze_heard(
            &self.root,
            &self.environment,
            &flag_layer(tests),
            adapters(),
            precise,
            progress,
        )
        .map_err(|error| error.to_string())
    }

    fn init(&self) -> Result<String, String> {
        init(&self.root).map_err(|error| error.to_string())
    }

    fn precise_ready(&self) -> Result<(), String> {
        precise_ready(&self.root, &self.environment)
    }

    fn open(&self, path: &str, line: u32) -> Result<(), String> {
        open_in_editor(&self.root, &self.environment, path, line)
    }

    fn copy(&self, text: &str) -> Result<(), String> {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(text))
            .map_err(|error| format!("The clipboard cannot be used: {error}"))
    }

    fn save_rule(
        &self,
        fields: &[(String, RawValue)],
        yours: bool,
        position: Option<usize>,
    ) -> Result<String, String> {
        save_rule(&self.root, &self.environment, fields, yours, position)
    }

    fn toggle_rule(&self, yours: bool, position: usize, enabled: bool) -> Result<(), String> {
        toggle_rule(&self.root, &self.environment, yours, position, enabled)
    }

    fn read(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(self.root.join(path))
            .map_err(|error| format!("{path} cannot be read: {error}"))
    }

    fn indexers(&self) -> Vec<IndexerReport> {
        indexer_reports(&self.root, &self.environment)
    }

    fn export(
        &self,
        format: &str,
        path: &str,
        view: &DiagramView,
        level: Level,
        overlay: Option<MetricId>,
    ) -> Result<String, String> {
        export_view(&self.root, format, path, view, level, overlay)
    }
}

/// Writes a diagram to a file of a project, in a format `cyclops export` knows.
///
/// # Errors
///
/// Says why not, such as a format it does not know.
pub fn export_view(
    root: &Path,
    format: &str,
    path: &str,
    view: &DiagramView,
    level: Level,
    overlay: Option<MetricId>,
) -> Result<String, String> {
    let known = ExportFormat::parse(format).ok_or_else(|| format!("Unknown format: {format}"))?;
    let written = Export {
        view,
        level,
        overlay,
    }
    .write(known)
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join(path), written)
        .map_err(|error| format!("{path} cannot be written: {error}"))?;
    Ok(format!("Wrote {path}."))
}

/// The settings file a rule goes to: your own, or the project's, and how to name it.
fn rule_file(
    root: &Path,
    environment: &Environment,
    yours: bool,
) -> Result<(PathBuf, String), String> {
    if yours {
        let user = UserConfig::locate(environment.home(), environment.xdg_config_home())
            .ok_or("There is no home folder for your own settings.")?;
        Ok((user.path, user.label))
    } else {
        Ok((
            ProjectFolder::of(root).config_path(),
            PROJECT_FILE.to_owned(),
        ))
    }
}

/// Saves an alert rule into your own settings file or the project's, keeping everything
/// else in it. Says where it went.
///
/// # Errors
///
/// Says why the file could not be changed.
pub fn save_rule(
    root: &Path,
    environment: &Environment,
    fields: &[(String, RawValue)],
    yours: bool,
    position: Option<usize>,
) -> Result<String, String> {
    let (path, label) = rule_file(root, environment, yours)?;
    save_alert(&path, position, fields)?;
    Ok(format!("Saved to {label}."))
}

/// Turns a rule of your own settings file or the project's on or off.
///
/// # Errors
///
/// Says why the file could not be changed.
pub fn toggle_rule(
    root: &Path,
    environment: &Environment,
    yours: bool,
    position: usize,
    enabled: bool,
) -> Result<(), String> {
    let (path, _) = rule_file(root, environment, yours)?;
    set_alert_enabled(&path, position, enabled)
}

/// Every indexer, and whether it can run here.
#[must_use]
pub fn indexer_reports(root: &Path, environment: &Environment) -> Vec<IndexerReport> {
    let tools = Tools::new(environment.get("PATH"));
    let cache = ProjectFolder::of(root).cache_path();
    providers(root, cache.parent().unwrap_or(root), &tools)
        .iter()
        .map(|provider| IndexerReport {
            label: provider.label().to_owned(),
            languages: provider.languages().to_vec(),
            indexer: provider.indexer().to_owned(),
            ready: provider.availability().map_err(|error| match error {
                IndexError::Missing(tool) => format!("{tool} is not installed."),
                IndexError::Failed(reason) => reason,
            }),
        })
        .collect()
}

impl Backend for Interactive {
    fn watch(&self, changed: Box<dyn Fn() -> bool + Send>) -> Result<Box<dyn Send>, String> {
        ProjectWatcher::start(&self.root, changed).map(|watcher| Box::new(watcher) as Box<dyn Send>)
    }
}

/// The editor of an environment: `$VISUAL`, or else `$EDITOR`.
#[must_use]
pub fn editor(environment: &Environment) -> Option<String> {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| environment.get(name))
        .map(str::trim)
        .find(|editor| !editor.is_empty())
        .map(str::to_owned)
}

/// Opens a file of a project in the editor at a line, and waits for it to close.
///
/// # Errors
///
/// Says why the editor could not run.
pub fn open_in_editor(
    root: &Path,
    environment: &Environment,
    path: &str,
    line: u32,
) -> Result<(), String> {
    let editor = editor(environment).ok_or("Set $EDITOR to open files.")?;
    let mut words = editor.split_whitespace();
    let program = words.next().ok_or("Set $EDITOR to open files.")?;
    let status = std::process::Command::new(program)
        .args(words)
        .arg(format!("+{line}"))
        .arg(path)
        .current_dir(root)
        .status()
        .map_err(|error| format!("{program} cannot run: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} ended with {status}"))
    }
}

/// Whether precise analysis can run on a project: it is not turned off, and an indexer
/// for one of its languages has the tools it needs.
///
/// # Errors
///
/// Says why it cannot, such as `rust-analyzer is not installed.`
pub fn precise_ready(root: &Path, environment: &Environment) -> Result<(), String> {
    let files = ProjectFiles::open(root).map_err(|error| error.to_string())?;
    let settings =
        settings(root, environment, &flag_layer(None), true).map_err(|error| error.to_string())?;
    if settings.semantic == SemanticMode::Off {
        return Err(Problem::PreciseOff.to_string());
    }
    let languages = analysis_service(adapters())
        .languages_in(&files)
        .map_err(|error| error.to_string())?;
    let tools = Tools::new(environment.get("PATH"));
    let folder = ProjectFolder::of(root);
    let cache = folder.cache_path();
    let mut missing = None;
    for provider in providers(root, cache.parent().unwrap_or(root), &tools) {
        if !provider
            .languages()
            .iter()
            .any(|language| languages.contains(language))
        {
            continue;
        }
        match provider.availability() {
            Ok(()) => return Ok(()),
            Err(error) => missing = missing.or(Some(error)),
        }
    }
    Err(match missing {
        Some(IndexError::Missing(tool)) => format!("{tool} is not installed."),
        Some(IndexError::Failed(reason)) => reason,
        None => "Nothing here can be indexed.".to_owned(),
    })
}

/// How the interface starts for a project and an environment: the theme from your own
/// settings, whether `NO_COLOR` is set, whether the project has settings of its own, and
/// whether there is an editor.
#[must_use]
pub fn interface_startup(root: &Path, environment: &Environment) -> Startup {
    let theme =
        match settings(root, environment, &flag_layer(None), true).map(|settings| settings.theme) {
            Ok(Theme::Dark) => ThemeChoice::Dark,
            Ok(Theme::Light) => ThemeChoice::Light,
            Ok(Theme::Auto) | Err(_) => ThemeChoice::Auto,
        };
    let no_color = environment
        .get("NO_COLOR")
        .is_some_and(|value| !value.is_empty());
    Startup {
        columns: 0,
        rows: 0,
        theme,
        background: None,
        no_color,
        initialized: ProjectFolder::of(root).has_config(),
        editor: editor(environment).is_some(),
    }
}

/// Opens the terminal interface.
fn interactive(root: &Path, environment: &Environment) -> Result<ExitCode> {
    ProjectFiles::open(root)?;
    let startup = interface_startup(root, environment);
    let backend: Arc<dyn Backend> = Arc::new(Interactive {
        root: root.to_path_buf(),
        environment: environment.clone(),
    });
    cyclops_tui::run(&backend, startup)?;
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
    let analysis = run_analysis(
        root,
        environment,
        &files,
        &settings,
        adapters(),
        false,
        &Silent,
    )?;
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
