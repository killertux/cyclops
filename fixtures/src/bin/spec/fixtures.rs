//! The fixtures used by the pages in `specs/`. See `specs/writing-specs.md`.

use crate::read::{self, NONE, NOT_APPLICABLE, list, yes_no};
use crate::scratch::Scratch;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::pipeline::AnalysisOptions;
use cyclops_cli::analyze;
use cyclops_domain::metric::{MetricId, Scope, Subject};
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::SymbolId;
use rust_slim::fixture;

/// The package every snippet is a part of.
const SNIPPET_PACKAGE: &str = "snippet";
const SNIPPET_MANIFEST: &str = "[package]\nname = \"snippet\"\nversion = \"0.1.0\"\n";
const SNIPPET_RUST_FILE: &str = "src/lib.rs";
const SNIPPET_TYPESCRIPT_FILE: &str = "src/snippet.ts";
const SNIPPET_JAVASCRIPT_FILE: &str = "src/snippet.js";

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
    fn run(&self) -> Outcome {
        let code = self.code.replace("\\n", "\n");
        let (manifest, path) = match self.language.as_str() {
            "rust" => (Some(SNIPPET_MANIFEST), SNIPPET_RUST_FILE),
            "typescript" => (None, SNIPPET_TYPESCRIPT_FILE),
            "javascript" => (None, SNIPPET_JAVASCRIPT_FILE),
            other => return Err(format!("unknown language: {other}")),
        };
        let scratch = Scratch::with_file(manifest, path, &code).map_err(|e| e.to_string())?;
        analyze(scratch.root(), AnalysisOptions::default()).map_err(|e| e.to_string())
    }

    fn id(&self, default: &str) -> SymbolId {
        let symbol = if self.symbol.is_empty() {
            default
        } else {
            &self.symbol
        };
        SymbolId::new(&format!("{SNIPPET_PACKAGE}::{symbol}"))
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
            Subject::File(RelativePath::new(SNIPPET_RUST_FILE))
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

    /// The package is the fresh project itself, so a snippet's own symbols have no parent.
    #[must_use]
    pub fn parent(&self) -> String {
        self.read(|analysis| {
            let parent = read::symbol(analysis, &self.id("f")).and_then(|s| s.parent.as_ref());
            parent
                .map(SymbolId::as_str)
                .and_then(|parent| parent.strip_prefix(SNIPPET_PACKAGE))
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
    outcome: Option<Outcome>,
}

impl Core {
    fn run(&self) -> Outcome {
        let scratch = Scratch::from_sample(&self.workspace).map_err(|e| e.to_string())?;
        let steps = self.steps.split(';').map(str::trim);
        for step in steps.filter(|step| !step.is_empty()) {
            scratch.step(step)?;
        }
        analyze(scratch.root(), AnalysisOptions::default()).map_err(|e| e.to_string())
    }

    fn read(&self, reader: impl FnOnce(&ProjectAnalysis) -> String) -> String {
        match &self.outcome {
            Some(Ok(analysis)) => reader(analysis),
            Some(Err(error)) => failed(error),
            None => not_run(),
        }
    }

    fn id(&self) -> SymbolId {
        SymbolId::new(&self.subject)
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

    pub fn execute(&mut self) {
        if !self.workspace.is_empty() {
            self.outcome = Some(self.run());
        }
    }

    #[must_use]
    pub fn analyzed(&self) -> String {
        yes_no(matches!(self.outcome, Some(Ok(_))))
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
