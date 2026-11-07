//! The fixtures used by the pages in `specs/`. See `specs/writing-specs.md`.

use crate::cli::{files_in, includes, run};
use crate::read::{self, NONE, NOT_APPLICABLE, list, yes_no};
use crate::scenario::{Scenario, no_cyclops};
use crate::scratch::Scratch;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::graph::ModuleGraph;
use cyclops_domain::metric::{MetricId, Scope, Subject};
use cyclops_domain::path::RelativePath;
use cyclops_domain::relation::{Confidence, RelationKind};
use cyclops_domain::settings::{SettingKey, Settings};
use cyclops_domain::symbol::SymbolId;
use rust_slim::fixture;
use std::process::{Command, Output};

/// The file a snippet is written to, and the module that makes it.
struct SnippetFile {
    manifest: Option<&'static str>,
    path: &'static str,
    module: &'static str,
}

const RUST_SNIPPET: SnippetFile = SnippetFile {
    manifest: Some("[package]\nname = \"snippet\"\nversion = \"0.1.0\"\n"),
    path: "src/lib.rs",
    module: "snippet",
};
const TYPESCRIPT_SNIPPET: SnippetFile = SnippetFile {
    manifest: None,
    path: "src/snippet.ts",
    module: "src/snippet",
};
const JAVASCRIPT_SNIPPET: SnippetFile = SnippetFile {
    manifest: None,
    path: "src/snippet.js",
    module: "src/snippet",
};

/// What a row reads from. A failure shows up in every column of its row.
type Outcome = Result<ProjectAnalysis, String>;

fn failed(error: &str) -> String {
    format!("failed: {error}")
}

fn not_run() -> String {
    failed("nothing was analyzed")
}

/// Analyzes one piece of code, as a single file of a fresh project.
#[derive(Default)]
pub struct Snippet {
    language: String,
    code: String,
    symbol: String,
    outcome: Option<Outcome>,
}

impl Snippet {
    fn file(&self) -> &'static SnippetFile {
        match self.language.as_str() {
            "typescript" => &TYPESCRIPT_SNIPPET,
            "javascript" => &JAVASCRIPT_SNIPPET,
            _ => &RUST_SNIPPET,
        }
    }

    fn run(&self) -> Outcome {
        let code = self.code.replace("\\n", "\n");
        let file = self.file();
        let scratch =
            Scratch::with_file(file.manifest, file.path, &code).map_err(|e| e.to_string())?;
        Scenario::new(scratch).analyze()
    }

    fn id(&self, default: &str) -> SymbolId {
        let symbol = if self.symbol.is_empty() {
            default
        } else {
            &self.symbol
        };
        SymbolId::new(&format!("{}::{symbol}", self.file().module))
    }

    fn read(&self, reader: impl FnOnce(&ProjectAnalysis) -> String) -> String {
        match &self.outcome {
            Some(Ok(analysis)) => reader(analysis),
            Some(Err(error)) => failed(error),
            None => not_run(),
        }
    }

    /// The whole file when there is no `symbol`, and otherwise that symbol.
    fn file_or_symbol(&self) -> Subject {
        if self.symbol.is_empty() {
            Subject::File(RelativePath::new(self.file().path))
        } else {
            Subject::Symbol(self.id(""))
        }
    }

    fn metric(&self, subject: &Subject, metric: MetricId) -> String {
        self.read(|analysis| read::metric_value(analysis, subject, metric))
    }

    fn of_function(&self, metric: MetricId) -> String {
        self.metric(&Subject::Symbol(self.id("f")), metric)
    }

    fn of_file_or_symbol(&self, metric: MetricId) -> String {
        self.metric(&self.file_or_symbol(), metric)
    }
}

#[fixture]
impl Snippet {
    #[slim(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn set_language(&mut self, language: String) {
        self.language = language;
    }

    pub fn set_code(&mut self, code: String) {
        self.code = code;
    }

    pub fn set_symbol(&mut self, symbol: String) {
        self.symbol = symbol;
    }

    pub fn execute(&mut self) {
        self.outcome = Some(self.run());
    }

    #[must_use]
    pub fn kind(&self) -> String {
        self.read(|analysis| {
            read::symbol(analysis, &self.id("f"))
                .map_or_else(|| NOT_APPLICABLE.into(), |symbol| symbol.kind.name().into())
        })
    }

    /// The module is the whole snippet, so what sits directly in it has no parent.
    #[must_use]
    pub fn parent(&self) -> String {
        self.read(|analysis| {
            let parent = read::symbol(analysis, &self.id("f")).and_then(|s| s.parent.as_ref());
            parent
                .map(SymbolId::as_str)
                .and_then(|parent| parent.strip_prefix(self.file().module))
                .and_then(|parent| parent.strip_prefix("::"))
                .unwrap_or(NONE)
                .into()
        })
    }

    #[must_use]
    pub fn found(&self) -> String {
        self.read(|analysis| yes_no(analysis.view().symbol(&self.id("f")).is_some()))
    }

    #[must_use]
    pub fn visibility(&self) -> String {
        self.read(|analysis| {
            read::symbol(analysis, &self.id("f")).map_or_else(
                || NOT_APPLICABLE.into(),
                |symbol| symbol.visibility.name().into(),
            )
        })
    }

    #[must_use]
    pub fn role(&self) -> String {
        self.read(|analysis| {
            read::symbol(analysis, &self.id("f"))
                .map_or_else(|| NOT_APPLICABLE.into(), |symbol| symbol.role.name().into())
        })
    }

    #[must_use]
    pub fn cyclomatic(&self) -> String {
        self.of_function(MetricId::Cyclomatic)
    }

    #[must_use]
    pub fn cognitive(&self) -> String {
        self.of_function(MetricId::Cognitive)
    }

    #[must_use]
    pub fn max_nesting(&self) -> String {
        self.of_function(MetricId::MaxNesting)
    }

    #[must_use]
    pub fn parameters(&self) -> String {
        self.of_function(MetricId::Parameters)
    }

    #[must_use]
    pub fn members(&self) -> String {
        self.metric(&Subject::Symbol(self.id("S")), MetricId::Members)
    }

    #[must_use]
    pub fn lcom(&self) -> String {
        self.metric(&Subject::Symbol(self.id("S")), MetricId::Lcom)
    }

    #[must_use]
    pub fn lines(&self) -> String {
        self.of_file_or_symbol(MetricId::Lines)
    }

    #[must_use]
    pub fn comment_lines(&self) -> String {
        self.of_file_or_symbol(MetricId::CommentLines)
    }

    #[must_use]
    pub fn blank_lines(&self) -> String {
        self.of_file_or_symbol(MetricId::BlankLines)
    }
}

/// Copies a sample workspace, takes the steps, analyzes it, and reads the result.
/// A row with no `workspace` only reads the catalog.
#[derive(Default)]
pub struct Core {
    workspace: String,
    steps: String,
    subject: String,
    metric: String,
    scope: String,
    value: String,
    setting: String,
    from: String,
    to: String,
    outcome: Option<Outcome>,
    settings: Option<Result<Settings, String>>,
}

impl Core {
    fn run(&mut self) -> Outcome {
        let scratch = Scratch::from_sample(&self.workspace).map_err(|e| e.to_string())?;
        let mut scenario = Scenario::new(scratch);
        scenario.steps(&self.steps, &no_cyclops)?;
        self.settings = Some(scenario.settings());
        scenario.analyze()
    }

    fn read(&self, reader: impl FnOnce(&ProjectAnalysis) -> String) -> String {
        match &self.outcome {
            Some(Ok(analysis)) => reader(analysis),
            Some(Err(error)) => failed(error),
            None => not_run(),
        }
    }

    fn read_settings(&self, key: SettingKey) -> String {
        match &self.settings {
            Some(Ok(settings)) => settings.display(key),
            Some(Err(error)) => failed(error),
            None => not_run(),
        }
    }

    fn id(&self) -> SymbolId {
        SymbolId::new(&self.subject)
    }

    fn subject_metric(&self, metric: MetricId) -> String {
        self.read(|analysis| read::metric_value(analysis, &read::subject(&self.subject), metric))
    }

    /// The relation between `from` and `to`. When there are several kinds, the first in
    /// the order of the relations page is read.
    fn relation_between(&self, analysis: &ProjectAnalysis) -> Option<(RelationKind, Confidence)> {
        let (from, to) = (SymbolId::new(&self.from), SymbolId::new(&self.to));
        analysis
            .view()
            .relations()
            .filter(|relation| relation.from == from && relation.to == to)
            .map(|relation| (relation.kind, relation.confidence))
            .min()
    }

    fn metric_id(&self) -> Result<MetricId, String> {
        MetricId::parse(&self.metric).ok_or_else(|| format!("unknown metric: {}", self.metric))
    }
}

#[fixture]
impl Core {
    #[slim(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn set_workspace(&mut self, workspace: String) {
        self.workspace = workspace;
    }

    pub fn set_steps(&mut self, steps: String) {
        self.steps = steps;
    }

    pub fn set_subject(&mut self, subject: String) {
        self.subject = subject;
    }

    pub fn set_metric(&mut self, metric: String) {
        self.metric = metric;
    }

    pub fn set_scope(&mut self, scope: String) {
        self.scope = scope;
    }

    pub fn set_value(&mut self, value: String) {
        self.value = value;
    }

    pub fn set_from(&mut self, from: String) {
        self.from = from;
    }

    pub fn set_to(&mut self, to: String) {
        self.to = to;
    }

    /// temoc calls a column that starts with `set` by its own name.
    pub fn setting(&mut self, setting: String) {
        self.setting = setting;
    }

    pub fn execute(&mut self) {
        if !self.workspace.is_empty() {
            self.outcome = Some(self.run());
        }
    }

    #[must_use]
    pub fn analyzed(&self) -> String {
        yes_no(matches!(self.outcome, Some(Ok(_))))
    }

    /// Why the analysis was refused.
    #[must_use]
    pub fn problem(&self) -> String {
        match &self.outcome {
            Some(Err(error)) => error.clone(),
            Some(Ok(_)) => NONE.into(),
            None => not_run(),
        }
    }

    #[must_use]
    pub fn problems(&self) -> String {
        self.read(|analysis| {
            let problems: Vec<String> = analysis
                .problems()
                .iter()
                .map(ToString::to_string)
                .collect();
            list(&problems)
        })
    }

    #[must_use]
    pub fn files(&self) -> String {
        self.read(|analysis| {
            let files: Vec<String> = analysis
                .files()
                .iter()
                .map(|file| file.path.to_string())
                .collect();
            list(&files)
        })
    }

    #[must_use]
    pub fn files_parsed(&self) -> String {
        self.read(|analysis| analysis.files_parsed().to_string())
    }

    #[must_use]
    pub fn language(&self) -> String {
        self.read(|analysis| {
            analysis
                .files()
                .iter()
                .find(|file| file.path.as_str() == self.subject)
                .map_or_else(|| NOT_APPLICABLE.into(), |file| file.language.to_string())
        })
    }

    #[must_use]
    pub fn found(&self) -> String {
        self.read(|analysis| yes_no(analysis.view().symbol(&self.id()).is_some()))
    }

    /// Whether a file or a symbol is test code, even when test code is left out.
    #[must_use]
    pub fn role(&self) -> String {
        self.read(|analysis| {
            let role = match read::subject(&self.subject) {
                Subject::File(path) => analysis
                    .files()
                    .iter()
                    .find(|file| file.path == path)
                    .map(|file| file.role),
                Subject::Symbol(id) => read::symbol(analysis, &id).map(|symbol| symbol.role),
            };
            role.map_or_else(|| NOT_APPLICABLE.into(), |role| role.name().into())
        })
    }

    #[must_use]
    pub fn kind(&self) -> String {
        self.read(|analysis| {
            analysis
                .view()
                .symbol(&self.id())
                .map_or_else(|| NOT_APPLICABLE.into(), |symbol| symbol.kind.name().into())
        })
    }

    #[must_use]
    pub fn parent(&self) -> String {
        self.read(|analysis| {
            let symbol = analysis.view().symbol(&self.id());
            let parent = symbol.and_then(|symbol| symbol.parent.as_ref());
            parent.map_or_else(|| NONE.into(), ToString::to_string)
        })
    }

    #[must_use]
    pub fn children(&self) -> String {
        self.read(|analysis| {
            let children = analysis.children(&self.id());
            let ids: Vec<String> = children.iter().map(|child| child.id.to_string()).collect();
            list(&ids)
        })
    }

    #[must_use]
    pub fn lines(&self) -> String {
        self.read(|analysis| {
            read::metric_value(analysis, &read::subject(&self.subject), MetricId::Lines)
        })
    }

    #[must_use]
    pub fn cyclomatic(&self) -> String {
        self.subject_metric(MetricId::Cyclomatic)
    }

    #[must_use]
    pub fn cognitive(&self) -> String {
        self.subject_metric(MetricId::Cognitive)
    }

    #[must_use]
    pub fn lcom(&self) -> String {
        self.subject_metric(MetricId::Lcom)
    }

    #[must_use]
    pub fn afferent(&self) -> String {
        self.subject_metric(MetricId::Afferent)
    }

    #[must_use]
    pub fn efferent(&self) -> String {
        self.subject_metric(MetricId::Efferent)
    }

    #[must_use]
    pub fn instability(&self) -> String {
        self.subject_metric(MetricId::Instability)
    }

    #[must_use]
    pub fn abstractness(&self) -> String {
        self.subject_metric(MetricId::Abstractness)
    }

    #[must_use]
    pub fn distance(&self) -> String {
        self.subject_metric(MetricId::Distance)
    }

    #[must_use]
    pub fn cycle_size(&self) -> String {
        self.subject_metric(MetricId::CycleSize)
    }

    #[must_use]
    pub fn in_cycle(&self) -> String {
        self.read(|analysis| {
            let cycles = ModuleGraph::of(&analysis.view()).cycles();
            let id = self.id();
            cycles.iter().any(|cycle| cycle.contains(&id)).to_string()
        })
    }

    /// Each cycle, with its members separated by commas and cycles by semicolons.
    #[must_use]
    pub fn cycles(&self) -> String {
        self.read(|analysis| {
            let cycles: Vec<String> = ModuleGraph::of(&analysis.view())
                .cycles()
                .iter()
                .map(|cycle| {
                    cycle
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect();
            if cycles.is_empty() {
                NONE.into()
            } else {
                cycles.join("; ")
            }
        })
    }

    #[must_use]
    pub fn relation(&self) -> String {
        self.read(|analysis| {
            self.relation_between(analysis)
                .map_or_else(|| NONE.into(), |(kind, _)| kind.name().into())
        })
    }

    #[must_use]
    pub fn confidence(&self) -> String {
        self.read(|analysis| {
            self.relation_between(analysis)
                .map_or_else(|| NONE.into(), |(_, confidence)| confidence.name().into())
        })
    }

    /// The value of the `setting` column.
    #[must_use]
    pub fn value(&self) -> String {
        if self.setting.is_empty() {
            return String::new();
        }
        SettingKey::parse(&self.setting).map_or_else(
            || failed(&format!("unknown setting {}", self.setting)),
            |key| self.read_settings(key),
        )
    }

    #[must_use]
    pub fn tests_included(&self) -> String {
        self.read_settings(SettingKey::TestsInclude)
    }

    #[must_use]
    pub fn direction(&self) -> String {
        self.metric_id().map_or_else(
            |error| failed(&error),
            |metric| metric.descriptor().direction.name().into(),
        )
    }

    /// `n/a` has no heat.
    #[must_use]
    pub fn heat(&self) -> String {
        let Ok(metric) = self.metric_id() else {
            return failed("unknown metric");
        };
        let Some(scope) = Scope::parse(&self.scope) else {
            return failed("unknown scope");
        };
        let heat = self
            .value
            .parse()
            .ok()
            .and_then(|value| metric.descriptor().heat_of(scope, value));
        heat.map_or_else(|| NONE.into(), |heat| heat.name().into())
    }
}

/// Runs the real `cyclops` binary, after the steps.
struct Ran {
    output: Output,
    created: Vec<String>,
    scenario: Scenario,
}

#[derive(Default)]
pub struct Cli {
    workspace: String,
    steps: String,
    command: String,
    subject: String,
    stdout_text: String,
    stderr_text: String,
    outcome: Option<Result<Ran, String>>,
}

impl Cli {
    fn play(&self) -> Result<Ran, String> {
        let scratch = Scratch::from_sample(&self.workspace).map_err(|e| e.to_string())?;
        let mut scenario = Scenario::new(scratch);
        let step_runner = |scenario: &Scenario, arguments: &str| {
            let output = run(scenario, arguments)?;
            if output.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).into_owned())
            }
        };
        scenario.steps(&self.steps, &step_runner)?;
        let before = files_in(scenario.root());
        let arguments = self
            .command
            .strip_prefix("cyclops")
            .ok_or_else(|| format!("not a cyclops command: {}", self.command))?;
        let output = run(&scenario, arguments)?;
        let created = files_in(scenario.root())
            .difference(&before)
            .cloned()
            .collect();
        Ok(Ran {
            output,
            created,
            scenario,
        })
    }

    fn read(&self, reader: impl FnOnce(&Ran) -> String) -> String {
        match &self.outcome {
            Some(Ok(ran)) => reader(ran),
            Some(Err(error)) => format!("failed: {error}"),
            None => "failed: nothing was run".into(),
        }
    }

    fn setting(&self, key: SettingKey) -> String {
        self.read(|ran| {
            ran.scenario.settings().map_or_else(
                |error| format!("failed: {error}"),
                |settings| settings.display(key),
            )
        })
    }
}

#[fixture]
impl Cli {
    #[slim(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn set_workspace(&mut self, workspace: String) {
        self.workspace = workspace;
    }

    pub fn set_steps(&mut self, steps: String) {
        self.steps = steps;
    }

    pub fn set_command(&mut self, command: String) {
        self.command = command;
    }

    pub fn set_subject(&mut self, subject: String) {
        self.subject = subject;
    }

    pub fn set_stdout_text(&mut self, text: String) {
        self.stdout_text = text;
    }

    pub fn set_stderr_text(&mut self, text: String) {
        self.stderr_text = text;
    }

    pub fn execute(&mut self) {
        self.outcome = Some(self.play());
    }

    #[must_use]
    pub fn exit_code(&self) -> String {
        self.read(|ran| {
            ran.output
                .status
                .code()
                .map_or_else(|| "killed".into(), |code| code.to_string())
        })
    }

    #[must_use]
    pub fn stdout_includes(&self) -> String {
        self.read(|ran| {
            includes(
                &String::from_utf8_lossy(&ran.output.stdout),
                &self.stdout_text,
            )
        })
    }

    #[must_use]
    pub fn stderr_includes(&self) -> String {
        self.read(|ran| {
            includes(
                &String::from_utf8_lossy(&ran.output.stderr),
                &self.stderr_text,
            )
        })
    }

    #[must_use]
    pub fn created(&self) -> String {
        self.read(|ran| list(&ran.created))
    }

    #[must_use]
    pub fn languages(&self) -> String {
        self.setting(SettingKey::ProjectLanguages)
    }

    #[must_use]
    pub fn tests_included(&self) -> String {
        self.setting(SettingKey::TestsInclude)
    }

    #[must_use]
    pub fn ignored_by_git(&self) -> String {
        self.read(|ran| {
            let checked = Command::new("git")
                .args(["check-ignore", "-q", &self.subject])
                .current_dir(ran.scenario.root())
                .status();
            match checked.map(|status| status.code()) {
                Ok(Some(0)) => yes_no(true),
                Ok(Some(1)) => yes_no(false),
                _ => NONE.into(),
            }
        })
    }
}
