//! The fixtures used by the pages in `specs/`. See `specs/writing-specs.md`.

use crate::cli::{files_in, includes, run};
use crate::read::{self, NONE, NOT_APPLICABLE, list, yes_no};
use crate::scenario::{Scenario, no_cyclops};
use crate::scratch::Scratch;
use crate::tui::Session;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::diagram::{DiagramRequest, DiagramView, Edge, Level};
use cyclops_application::graph::ModuleGraph;
use cyclops_application::queries::{Explanation, Order};
use cyclops_domain::filter::Filter;
use cyclops_domain::metric::{MetricId, Scope, Subject};
use cyclops_domain::path::RelativePath;
use cyclops_domain::relation::{Confidence, RelationKind};
use cyclops_domain::settings::Severity;
use cyclops_domain::settings::{SettingKey, Settings};
use cyclops_domain::symbol::SymbolId;
use cyclops_domain::symbol::SymbolKind;
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
    pub fn kinds(&self) -> String {
        self.read(|analysis| {
            analysis.explain(&self.id("f")).map_or_else(
                || NONE.into(),
                |explanation| {
                    let kinds: Vec<String> = explanation
                        .kinds
                        .iter()
                        .map(|(kind, count)| format!("{kind} {count}"))
                        .collect();
                    list(&kinds)
                },
            )
        })
    }

    #[must_use]
    pub fn groups(&self) -> String {
        self.read(|analysis| read::method_groups(analysis, &self.id("S"), false))
    }

    #[must_use]
    pub fn group_fields(&self) -> String {
        self.read(|analysis| read::method_groups(analysis, &self.id("S"), true))
    }

    #[must_use]
    pub fn stereotype(&self) -> String {
        self.read(|analysis| {
            read::symbol(analysis, &self.id("S")).map_or_else(
                || NOT_APPLICABLE.into(),
                |symbol| match symbol.kind {
                    SymbolKind::Interface => "interface".into(),
                    SymbolKind::Enum => "enum".into(),
                    SymbolKind::Alias => "alias".into(),
                    _ => NONE.into(),
                },
            )
        })
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
    filter: String,
    sort: String,
    limit: String,
    alert: String,
    distance: String,
    level: String,
    focus: String,
    depth: String,
    relations: String,
    overlay: String,
    outcome: Option<Outcome>,
    settings: Option<Result<Settings, String>>,
    indexers_ran: bool,
}

impl Core {
    fn run(&mut self) -> Outcome {
        let scratch = Scratch::from_sample(&self.workspace).map_err(|e| e.to_string())?;
        let mut scenario = Scenario::new(scratch);
        scenario.steps(&self.steps, &no_cyclops)?;
        for rule in self
            .alert
            .split(';')
            .map(str::trim)
            .filter(|rule| !rule.is_empty())
        {
            scenario.steps(&format!("add alert {rule}"), &no_cyclops)?;
        }
        self.settings = Some(scenario.settings());
        let analysis = scenario.analyze();
        self.indexers_ran = scenario.indexers_ran();
        analysis
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

    fn parsed_filter(&self) -> Result<Option<Filter>, String> {
        if self.filter.is_empty() {
            return Ok(None);
        }
        Filter::parse(&self.filter)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    fn matching(&self, analysis: &ProjectAnalysis) -> Result<Vec<String>, String> {
        let Some(filter) = self.parsed_filter()? else {
            return Err("no filter".into());
        };
        let sort = match self.sort.split_once(' ') {
            Some((metric, order)) => {
                let metric = MetricId::parse(metric).ok_or("unknown metric")?;
                let order = if order == "asc" {
                    Order::Ascending
                } else {
                    Order::Descending
                };
                Some((metric, order))
            }
            None => None,
        };
        let limit = self.limit.parse().ok();
        Ok(analysis
            .search(&filter, sort, limit)
            .iter()
            .map(|symbol| symbol.id.to_string())
            .collect())
    }

    fn request(&self) -> Result<DiagramRequest, String> {
        let level =
            Level::parse(&self.level).ok_or_else(|| format!("unknown level {}", self.level))?;
        let relations = self
            .relations
            .split(',')
            .map(str::trim)
            .filter(|kind| !kind.is_empty())
            .map(|kind| RelationKind::parse(kind).ok_or_else(|| format!("unknown relation {kind}")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DiagramRequest {
            level,
            focus: (!self.focus.is_empty()).then(|| SymbolId::new(&self.focus)),
            depth: self.depth.parse().unwrap_or(0),
            relations,
            overlay: MetricId::parse(&self.overlay),
            filter: self.parsed_filter()?,
        })
    }

    /// Reads something of the diagram the row asks for.
    fn drawn(&self, reader: impl FnOnce(&DiagramView) -> String) -> String {
        self.read(|analysis| match self.request() {
            Ok(request) => reader(&analysis.diagram(&request)),
            Err(error) => failed(&error),
        })
    }

    fn edge<'d>(&self, view: &'d DiagramView) -> Option<&'d Edge> {
        let (from, to) = (SymbolId::new(&self.from), SymbolId::new(&self.to));
        view.edges
            .iter()
            .filter(|edge| edge.from == from && edge.to == to)
            .min_by_key(|edge| edge.kind)
    }

    fn explained(&self, reader: impl FnOnce(&Explanation) -> String) -> String {
        self.read(|analysis| {
            analysis
                .explain(&self.id())
                .map_or_else(|| NONE.into(), |explanation| reader(&explanation))
        })
    }

    fn impact_count(&self, confidence: Confidence) -> String {
        self.read(|analysis| {
            let impact = analysis.impact(&self.id());
            impact
                .all()
                .filter(|(_, sure)| *sure == confidence)
                .count()
                .to_string()
        })
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

    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }

    pub fn set_sort(&mut self, sort: String) {
        self.sort = sort;
    }

    pub fn set_limit(&mut self, limit: String) {
        self.limit = limit;
    }

    pub fn set_alert(&mut self, alert: String) {
        self.alert = alert;
    }

    pub fn set_distance(&mut self, distance: String) {
        self.distance = distance;
    }

    pub fn set_level(&mut self, level: String) {
        self.level = level;
    }

    pub fn set_focus(&mut self, focus: String) {
        self.focus = focus;
    }

    pub fn set_depth(&mut self, depth: String) {
        self.depth = depth;
    }

    pub fn set_relations(&mut self, relations: String) {
        self.relations = relations;
    }

    pub fn set_overlay(&mut self, overlay: String) {
        self.overlay = overlay;
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

    /// Whether any analysis of the row ran an indexer.
    #[must_use]
    pub fn indexers_run(&self) -> String {
        match &self.outcome {
            Some(_) => yes_no(self.indexers_ran),
            None => not_run(),
        }
    }

    /// Whether the indexes still match the code. Without any, they do not.
    #[must_use]
    pub fn index_current(&self) -> String {
        self.read(|analysis| yes_no(analysis.precise().current == Some(true)))
    }

    /// Why the analysis or the filter was refused.
    #[must_use]
    pub fn problem(&self) -> String {
        match &self.outcome {
            Some(Err(error)) => error.clone(),
            Some(Ok(_)) => self.parsed_filter().err().unwrap_or_else(|| NONE.into()),
            None => not_run(),
        }
    }

    #[must_use]
    pub fn results(&self) -> String {
        self.read(|analysis| {
            self.matching(analysis)
                .map_or_else(|error| failed(&error), |found| list(&found))
        })
    }

    #[must_use]
    pub fn kept(&self) -> String {
        self.read(|analysis| match self.parsed_filter() {
            Ok(Some(filter)) => yes_no(analysis.kept(&filter, &self.id())),
            Ok(None) => failed("no filter"),
            Err(error) => failed(&error),
        })
    }

    #[must_use]
    pub fn alerts(&self) -> String {
        self.read(|analysis| {
            let alerts: Vec<String> = analysis
                .alerts()
                .iter()
                .map(|alert| {
                    let severity = if alert.severity == Severity::Error {
                        "error"
                    } else {
                        "warning"
                    };
                    format!("{severity} {}", alert.subject)
                })
                .collect();
            list(&alerts)
        })
    }

    #[must_use]
    pub fn alerted(&self) -> String {
        self.read(|analysis| {
            let subject = read::subject(&self.subject);
            yes_no(
                analysis
                    .alerts()
                    .iter()
                    .any(|alert| alert.subject == subject),
            )
        })
    }

    #[must_use]
    pub fn rank(&self) -> String {
        self.read(|analysis| {
            let rank = self
                .metric_id()
                .ok()
                .and_then(|metric| analysis.rank(&read::subject(&self.subject), metric));
            rank.map_or_else(|| NOT_APPLICABLE.into(), |rank| rank.position.to_string())
        })
    }

    #[must_use]
    pub fn of(&self) -> String {
        self.read(|analysis| {
            let rank = self
                .metric_id()
                .ok()
                .and_then(|metric| analysis.rank(&read::subject(&self.subject), metric));
            rank.map_or_else(|| NOT_APPLICABLE.into(), |rank| rank.of.to_string())
        })
    }

    #[must_use]
    pub fn spread(&self) -> String {
        self.read(|analysis| {
            let spread = self
                .metric_id()
                .ok()
                .zip(Scope::parse(&self.level))
                .and_then(|(metric, scope)| analysis.spread(scope, metric));
            spread.map_or_else(
                || NONE.into(),
                |counts| {
                    counts
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            )
        })
    }

    /// Up to three names, then how many more tie with them.
    #[must_use]
    pub fn worst(&self) -> String {
        self.read(|analysis| {
            let Some(worst) = self
                .metric_id()
                .ok()
                .and_then(|metric| analysis.worst(metric))
            else {
                return NONE.into();
            };
            let names: Vec<String> = worst
                .subjects
                .iter()
                .take(3)
                .map(ToString::to_string)
                .collect();
            let more = worst.subjects.len().saturating_sub(3);
            if more > 0 {
                format!("{} and {more} more", names.join(", "))
            } else {
                names.join(", ")
            }
        })
    }

    #[must_use]
    pub fn dependents(&self) -> String {
        self.read(|analysis| {
            let distance: usize = self.distance.parse().unwrap_or(1);
            let impact = analysis.impact(&self.id());
            let level = distance
                .checked_sub(1)
                .and_then(|index| impact.levels.get(index));
            let ids: Vec<String> = level
                .into_iter()
                .flatten()
                .map(|(id, _)| id.to_string())
                .collect();
            list(&ids)
        })
    }

    #[must_use]
    pub fn exact(&self) -> String {
        self.impact_count(Confidence::Exact)
    }

    #[must_use]
    pub fn inferred(&self) -> String {
        self.impact_count(Confidence::Inferred)
    }

    #[must_use]
    pub fn ambiguous(&self) -> String {
        self.impact_count(Confidence::Ambiguous)
    }

    #[must_use]
    pub fn reach(&self) -> String {
        self.read(|analysis| analysis.impact(&self.id()).all().count().to_string())
    }

    #[must_use]
    pub fn packages(&self) -> String {
        self.read(|analysis| {
            let view = analysis.view();
            let packages: std::collections::BTreeSet<SymbolId> = analysis
                .impact(&self.id())
                .all()
                .filter_map(|(id, _)| {
                    let mut current = view.symbol(id);
                    while let Some(symbol) = current {
                        if symbol.kind == SymbolKind::Package {
                            return Some(symbol.id.clone());
                        }
                        current = symbol
                            .parent
                            .as_ref()
                            .and_then(|parent| view.symbol(parent));
                    }
                    None
                })
                .collect();
            packages.len().to_string()
        })
    }

    #[must_use]
    pub fn points(&self) -> String {
        self.explained(|explanation| {
            let points: Vec<String> = explanation
                .points
                .iter()
                .map(|(line, points)| format!("{line} +{points}"))
                .collect();
            list(&points)
        })
    }

    #[must_use]
    pub fn decisions(&self) -> String {
        self.explained(|explanation| explanation.decisions.to_string())
    }

    #[must_use]
    pub fn nesting(&self) -> String {
        self.explained(|explanation| explanation.nesting.to_string())
    }

    #[must_use]
    pub fn points_by_kind(&self) -> String {
        self.explained(|explanation| {
            let kinds: Vec<String> = explanation
                .points_by_kind
                .iter()
                .map(|(kind, points)| format!("{kind} {points}"))
                .collect();
            list(&kinds)
        })
    }

    #[must_use]
    pub fn deepest_line(&self) -> String {
        self.explained(|explanation| {
            explanation
                .deepest
                .as_ref()
                .map_or_else(|| NONE.into(), |(line, _)| line.to_string())
        })
    }

    #[must_use]
    pub fn deepest_path(&self) -> String {
        self.explained(|explanation| {
            explanation
                .deepest
                .as_ref()
                .map_or_else(|| NONE.into(), |(_, path)| path.join(", "))
        })
    }

    #[must_use]
    pub fn in_file(&self) -> String {
        self.read(|analysis| {
            let rank = self
                .metric_id()
                .ok()
                .and_then(|metric| analysis.in_file(&self.id(), metric));
            rank.map_or_else(
                || NOT_APPLICABLE.into(),
                |rank| format!("{} of {}", rank.position, rank.of),
            )
        })
    }

    #[must_use]
    pub fn commits(&self) -> String {
        self.subject_metric(MetricId::Churn)
    }

    #[must_use]
    pub fn file_complexity(&self) -> String {
        self.subject_metric(MetricId::Cyclomatic)
    }

    #[must_use]
    pub fn latest_commit(&self) -> String {
        self.read(|analysis| {
            let commits = analysis.commits_of(&RelativePath::new(&self.subject));
            commits
                .first()
                .map_or_else(|| NONE.into(), |commit| commit.subject.clone())
        })
    }

    #[must_use]
    pub fn groups(&self) -> String {
        self.read(|analysis| read::method_groups(analysis, &self.id(), false))
    }

    #[must_use]
    pub fn depends_on(&self) -> String {
        self.read(|analysis| {
            let graph = ModuleGraph::of(&analysis.view());
            let found: Vec<String> = graph
                .depends_on(&self.id())
                .iter()
                .map(ToString::to_string)
                .collect();
            list(&found)
        })
    }

    #[must_use]
    pub fn depended_on_by(&self) -> String {
        self.read(|analysis| {
            let graph = ModuleGraph::of(&analysis.view());
            let found: Vec<String> = graph
                .depended_on_by(&self.id())
                .iter()
                .map(ToString::to_string)
                .collect();
            list(&found)
        })
    }

    #[must_use]
    pub fn cycle_path(&self) -> String {
        self.read(|analysis| {
            let Some(hops) = analysis.cycle_path(&self.id()) else {
                return NONE.into();
            };
            let mut path: Vec<String> = hops.iter().map(|(from, _, _)| from.to_string()).collect();
            path.extend(hops.last().map(|(_, to, _)| to.to_string()));
            path.join(" -> ")
        })
    }

    #[must_use]
    pub fn cycle_hops(&self) -> String {
        self.read(|analysis| {
            let Some(hops) = analysis.cycle_path(&self.id()) else {
                return NONE.into();
            };
            hops.iter()
                .map(|(from, to, confidence)| {
                    format!("{from} -> {to} imports {}", confidence.name())
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    #[must_use]
    pub fn nodes(&self) -> String {
        self.drawn(|view| {
            let ids: Vec<String> = view.nodes.iter().map(|node| node.id.to_string()).collect();
            list(&ids)
        })
    }

    #[must_use]
    pub fn edges(&self) -> String {
        self.drawn(|view| {
            let edges: Vec<String> = view
                .edges
                .iter()
                .map(|edge| format!("{} -> {} ({})", edge.from, edge.to, edge.kind.name()))
                .collect();
            list(&edges)
        })
    }

    #[must_use]
    pub fn overlay_value(&self) -> String {
        let metric = MetricId::parse(&self.overlay);
        self.drawn(|view| {
            let node = view.node(&self.id());
            match (node.and_then(|node| node.overlay), metric) {
                (Some(value), Some(metric)) => metric.descriptor().format(value),
                _ => NOT_APPLICABLE.into(),
            }
        })
    }

    #[must_use]
    pub fn member_value(&self) -> String {
        let Some(metric) = MetricId::parse(&self.overlay) else {
            return failed("no overlay");
        };
        self.subject_metric(metric)
    }

    #[must_use]
    pub fn edge_confidence(&self) -> String {
        self.drawn(|view| {
            self.edge(view)
                .map_or_else(|| NONE.into(), |edge| edge.confidence.name().into())
        })
    }

    #[must_use]
    pub fn edge_in_cycle(&self) -> String {
        self.drawn(|view| {
            self.edge(view)
                .map_or_else(|| NONE.into(), |edge| yes_no(edge.in_cycle))
        })
    }

    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "temoc names this method after the `from members?` column"
    )]
    pub fn from_members(&self) -> String {
        self.drawn(|view| {
            self.edge(view)
                .map_or_else(|| NONE.into(), |edge| list(&edge.from_members))
        })
    }

    #[must_use]
    pub fn to_members(&self) -> String {
        self.drawn(|view| {
            self.edge(view)
                .map_or_else(|| NONE.into(), |edge| list(&edge.to_members))
        })
    }

    #[must_use]
    pub fn stereotype(&self) -> String {
        self.drawn(|view| {
            view.node(&self.id()).map_or_else(
                || NOT_APPLICABLE.into(),
                |node| node.stereotype.unwrap_or(NONE).into(),
            )
        })
    }

    #[must_use]
    pub fn fields(&self) -> String {
        self.drawn(|view| {
            let names: Vec<String> = view
                .node(&self.id())
                .map(|node| {
                    node.fields
                        .iter()
                        .map(|member| member.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            list(&names)
        })
    }

    #[must_use]
    pub fn methods(&self) -> String {
        self.drawn(|view| {
            let names: Vec<String> = view
                .node(&self.id())
                .map(|node| {
                    node.methods
                        .iter()
                        .map(|member| member.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            list(&names)
        })
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
    pub fn churn(&self) -> String {
        self.subject_metric(MetricId::Churn)
    }

    #[must_use]
    pub fn hotspot(&self) -> String {
        self.subject_metric(MetricId::Hotspot)
    }

    #[must_use]
    pub fn duplication(&self) -> String {
        self.subject_metric(MetricId::Duplication)
    }

    #[must_use]
    pub fn unavailable(&self) -> String {
        self.read(|analysis| {
            let unavailable: Vec<String> = analysis
                .unavailable()
                .iter()
                .map(ToString::to_string)
                .collect();
            list(&unavailable)
        })
    }

    #[must_use]
    pub fn commits_read(&self) -> String {
        self.read(|analysis| analysis.commits_read().to_string())
    }

    /// Each group of files that share a copy, separated by semicolons.
    #[must_use]
    pub fn clones(&self) -> String {
        self.read(|analysis| {
            let groups: Vec<String> = analysis
                .duplication()
                .groups
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect();
            if groups.is_empty() {
                NONE.into()
            } else {
                groups.join("; ")
            }
        })
    }

    /// Where the copies of the subject are, as `FILE:FIRST-LAST`.
    #[must_use]
    pub fn copies(&self) -> String {
        self.read(|analysis| {
            let path = RelativePath::new(&self.subject);
            let partners = analysis.duplication().copies.get(&path);
            let copies: Vec<String> = partners
                .into_iter()
                .flatten()
                .flat_map(|(file, ranges)| {
                    ranges
                        .iter()
                        .map(move |(first, last)| format!("{file}:{first}-{last}"))
                })
                .collect();
            list(&copies)
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

    /// The value of the `setting` column, or the worst value of the `metric` column.
    #[must_use]
    pub fn value(&self) -> String {
        if self.setting.is_empty() && !self.metric.is_empty() {
            return self.read(|analysis| {
                self.metric_id()
                    .ok()
                    .and_then(|metric| {
                        analysis
                            .worst(metric)
                            .map(|worst| metric.descriptor().format(worst.value))
                    })
                    .unwrap_or_else(|| NOT_APPLICABLE.into())
            });
        }
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
    pub fn valid_json(&self) -> String {
        self.read(|ran| {
            yes_no(crate::json::is_valid(&String::from_utf8_lossy(
                &ran.output.stdout,
            )))
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

/// Drives the terminal interface, without a terminal. See `specs/tui/`.
#[derive(Default)]
pub struct Tui {
    workspace: String,
    steps: String,
    size: String,
    keys: String,
    subject: String,
    text: String,
    other_text: String,
    config_text: String,
    other_config_text: String,
    user_config_text: String,
    exported_text: String,
    language: String,
    outcome: Option<Result<Session, String>>,
    screen: String,
}

impl Tui {
    fn run(&self) -> Result<Session, String> {
        let mut session = Session::open(&self.workspace, &self.steps, &self.size)?;
        session.press(&self.keys)?;
        Ok(session)
    }

    fn read(&self, reader: impl FnOnce(&Session) -> String) -> String {
        match &self.outcome {
            Some(Ok(session)) => reader(session),
            Some(Err(error)) => failed(error),
            None => failed("the interface was not opened"),
        }
    }

    fn config(session: &Session) -> String {
        std::fs::read_to_string(session.scenario.root().join(".cyclops/config.toml"))
            .unwrap_or_default()
    }

    fn id_or_none(id: Option<&SymbolId>) -> String {
        id.map_or_else(|| NONE.into(), ToString::to_string)
    }
}

#[fixture]
impl Tui {
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

    pub fn set_size(&mut self, size: String) {
        self.size = size;
    }

    pub fn set_keys(&mut self, keys: String) {
        self.keys = keys;
    }

    pub fn set_subject(&mut self, subject: String) {
        self.subject = subject;
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    pub fn set_other_text(&mut self, text: String) {
        self.other_text = text;
    }

    pub fn set_config_text(&mut self, text: String) {
        self.config_text = text;
    }

    pub fn set_other_config_text(&mut self, text: String) {
        self.other_config_text = text;
    }

    pub fn set_user_config_text(&mut self, text: String) {
        self.user_config_text = text;
    }

    pub fn set_language(&mut self, language: String) {
        self.language = language;
    }

    pub fn set_exported_text(&mut self, text: String) {
        self.exported_text = text;
    }

    /// Whether the file an export wrote has the `exported text`.
    #[must_use]
    pub fn exported_includes(&self) -> String {
        self.read(|session| {
            let written = crate::cli::files_in(session.scenario.root())
                .difference(&session.before)
                .find(|path| !path.starts_with(".cyclops"))
                .and_then(|path| std::fs::read_to_string(session.scenario.root().join(path)).ok())
                .unwrap_or_default();
            includes(&written, &self.exported_text)
        })
    }

    /// The tile with the most lines.
    #[must_use]
    pub fn largest(&self) -> String {
        self.read(|session| Self::id_or_none(session.app.largest_tile().as_ref()))
    }

    /// The value on the `subject` tile.
    #[must_use]
    pub fn tile_value(&self) -> String {
        self.read(|session| {
            let subject = SymbolId::new(&self.subject);
            if session.app.tiles().iter().any(|tile| tile.id == subject) {
                session.app.shown_value(&subject, session.app.lens().metric)
            } else {
                NONE.into()
            }
        })
    }

    /// The value on the `subject` node of the diagram.
    #[must_use]
    pub fn node_value(&self) -> String {
        self.read(|session| {
            let subject = SymbolId::new(&self.subject);
            let shown = session
                .app
                .drawing()
                .is_some_and(|drawing| drawing.view.node(&subject).is_some());
            if shown {
                session.app.shown_value(&subject, session.app.lens().metric)
            } else {
                NONE.into()
            }
        })
    }

    /// The overlay on the `subject` member of a class box.
    #[must_use]
    pub fn member_value(&self) -> String {
        self.read(|session| {
            let subject = SymbolId::new(&self.subject);
            session
                .app
                .scene()
                .and_then(|(_, scene)| {
                    scene.cards.iter().find_map(|card| {
                        card.rows.iter().find_map(|row| match row {
                            cyclops_tui::scene::BoxRow::Member { id, value, .. }
                                if *id == subject =>
                            {
                                let metric = session.app.lens().metric;
                                Some(value.map_or_else(
                                    || NOT_APPLICABLE.to_owned(),
                                    |value| metric.descriptor().format(value),
                                ))
                            }
                            _ => None,
                        })
                    })
                })
                .unwrap_or_else(|| NONE.into())
        })
    }

    /// The relation kinds the diagram draws, or `all`.
    #[must_use]
    pub fn relations(&self) -> String {
        self.read(|session| {
            let kinds = session.app.diagram_relations();
            if kinds.is_empty() {
                "all".to_owned()
            } else {
                let names: Vec<String> = kinds.iter().map(|kind| kind.name().to_owned()).collect();
                names.join(", ")
            }
        })
    }

    /// The width of the inspector, or `closed`.
    #[must_use]
    pub fn inspector_width(&self) -> String {
        self.read(|session| {
            session
                .app
                .inspector_width()
                .map_or_else(|| "closed".to_owned(), |width| width.to_string())
        })
    }

    #[must_use]
    pub fn other_config_includes(&self) -> String {
        self.read(|session| includes(&Self::config(session), &self.other_config_text))
    }

    #[must_use]
    pub fn user_config_includes(&self) -> String {
        self.read(|session| {
            let text = session
                .scenario
                .user_config_file()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .unwrap_or_default();
            includes(&text, &self.user_config_text)
        })
    }

    /// The alert rules Alerts lists, in order.
    #[must_use]
    pub fn rules(&self) -> String {
        self.read(|session| {
            let rules: Vec<String> = session
                .app
                .listed_rules()
                .iter()
                .map(|(_, rule)| rule.title())
                .collect();
            list(&rules)
        })
    }

    /// How many symbols the rule being written would fire on.
    #[must_use]
    pub fn hits(&self) -> String {
        self.read(|session| session.app.editor_hits().to_string())
    }

    /// The rule being written, as `METRIC SCOPE WHEN SEVERITY`.
    #[must_use]
    pub fn rule(&self) -> String {
        self.read(|session| {
            session
                .app
                .editor()
                .map_or_else(|| NONE.into(), cyclops_tui::editor::RuleEditor::summary)
        })
    }

    /// The field of the rule being written.
    #[must_use]
    pub fn field(&self) -> String {
        self.read(|session| {
            session
                .app
                .editor()
                .map_or(NONE, cyclops_tui::editor::RuleEditor::field_name)
                .to_owned()
        })
    }

    /// The lines of Why's source with points, and those points.
    #[must_use]
    pub fn gutter(&self) -> String {
        self.read(|session| {
            let points: Vec<String> = session
                .app
                .gutter()
                .iter()
                .map(|(line, points)| format!("{line} +{points}"))
                .collect();
            list(&points)
        })
    }

    /// The current line of Why's source.
    #[must_use]
    pub fn line(&self) -> String {
        self.read(|session| session.app.why_line().to_string())
    }

    /// Whether the indexer of the `language` can run.
    #[must_use]
    pub fn ready(&self) -> String {
        self.read(|session| {
            session
                .app
                .indexers()
                .iter()
                .find(|report| {
                    report
                        .languages
                        .iter()
                        .any(|language| language.name() == self.language)
                })
                .map_or_else(|| NONE.into(), |report| yes_no(report.ready.is_ok()))
        })
    }

    pub fn execute(&mut self) {
        if self.workspace.is_empty() {
            return;
        }
        let mut outcome = self.run();
        if let Ok(session) = &mut outcome {
            self.screen = session.screen();
        }
        self.outcome = Some(outcome);
    }

    #[must_use]
    pub fn includes(&self) -> String {
        self.read(|_| includes(&self.screen, &self.text))
    }

    #[must_use]
    pub fn other_includes(&self) -> String {
        self.read(|_| includes(&self.screen, &self.other_text))
    }

    #[must_use]
    pub fn config_includes(&self) -> String {
        self.read(|session| includes(&Self::config(session), &self.config_text))
    }

    #[must_use]
    pub fn view(&self) -> String {
        self.read(|session| session.app.view().name().to_owned())
    }

    #[must_use]
    pub fn mode(&self) -> String {
        self.read(|session| session.app.mode().name().to_owned())
    }

    #[must_use]
    pub fn focus(&self) -> String {
        self.read(|session| Self::id_or_none(session.app.focus()))
    }

    #[must_use]
    pub fn selected(&self) -> String {
        self.read(|session| Self::id_or_none(session.app.selected().as_ref()))
    }

    #[must_use]
    pub fn inspecting(&self) -> String {
        self.read(|session| {
            let shown = session.app.inspector().and(session.app.selected());
            Self::id_or_none(shown.as_ref())
        })
    }

    #[must_use]
    pub fn metric(&self) -> String {
        self.read(|session| session.app.metric_label().to_owned())
    }

    #[must_use]
    pub fn filter(&self) -> String {
        self.read(|session| {
            session
                .app
                .lens()
                .filter
                .as_ref()
                .map_or_else(|| NONE.into(), |filter| filter.text.clone())
        })
    }

    #[must_use]
    pub fn tests(&self) -> String {
        self.read(|session| {
            if session.app.lens().tests {
                "on"
            } else {
                "off"
            }
            .into()
        })
    }

    #[must_use]
    pub fn popup(&self) -> String {
        self.read(|session| session.app.popup_name().unwrap_or(NONE).to_owned())
    }

    #[must_use]
    pub fn popup_keys(&self) -> String {
        self.read(|session| list(&session.app.popup_keys()))
    }

    #[must_use]
    pub fn rows(&self) -> String {
        self.read(|session| {
            let rows: Vec<String> = match session.app.view() {
                cyclops_tui::state::View::Alerts => session.app.alert_rows(),
                cyclops_tui::state::View::Status => session
                    .app
                    .analysis()
                    .map(|analysis| {
                        analysis
                            .problems()
                            .iter()
                            .map(ToString::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                cyclops_tui::state::View::Find => session
                    .app
                    .found()
                    .iter()
                    .map(|symbol| symbol.id.to_string())
                    .collect(),
                _ => session
                    .app
                    .tree()
                    .iter()
                    .map(|row| row.symbol.id.to_string())
                    .collect(),
            };
            list(&rows)
        })
    }

    #[must_use]
    pub fn count(&self) -> String {
        self.read(|session| session.app.found().len().to_string())
    }

    #[must_use]
    pub fn nodes(&self) -> String {
        self.read(|session| {
            let mut nodes: Vec<String> = session
                .app
                .drawing()
                .map(|drawing| {
                    drawing
                        .view
                        .nodes
                        .iter()
                        .map(|node| node.id.to_string())
                        .collect()
                })
                .unwrap_or_default();
            nodes.sort();
            list(&nodes)
        })
    }

    #[must_use]
    pub fn level(&self) -> String {
        self.read(|session| {
            session
                .app
                .drawing()
                .filter(|_| session.app.view() == cyclops_tui::state::View::Diagram)
                .map_or_else(
                    || NONE.into(),
                    |drawing| {
                        match drawing.level {
                            Level::Package => "package",
                            Level::Module => "module",
                            Level::Type => "type",
                            Level::Member => "member",
                        }
                        .into()
                    },
                )
        })
    }

    #[must_use]
    pub fn depth(&self) -> String {
        self.read(|session| {
            session
                .app
                .drawing()
                .filter(|_| session.app.view() == cyclops_tui::state::View::Diagram)
                .map_or_else(|| NONE.into(), |drawing| drawing.depth.to_string())
        })
    }

    #[must_use]
    pub fn tiles(&self) -> String {
        self.read(|session| {
            let tiles: Vec<String> = session
                .app
                .tiles()
                .iter()
                .map(|symbol| symbol.id.to_string())
                .collect();
            list(&tiles)
        })
    }

    #[must_use]
    pub fn panes(&self) -> String {
        self.read(|session| {
            let panes: Vec<String> = session
                .app
                .panes()
                .iter()
                .map(|pane| pane.name().to_owned())
                .collect();
            list(&panes)
        })
    }

    #[must_use]
    pub fn pane(&self) -> String {
        self.read(|session| {
            session
                .app
                .pane()
                .map_or(NONE, |pane| pane.name())
                .to_owned()
        })
    }

    #[must_use]
    pub fn section(&self) -> String {
        self.read(|session| {
            session
                .app
                .inspector()
                .map_or("closed", |section| section.name())
                .to_owned()
        })
    }

    #[must_use]
    pub fn trail(&self) -> String {
        self.read(|session| {
            let (position, length) = session.app.trail();
            format!("{position}/{length}")
        })
    }

    #[must_use]
    pub fn running(&self) -> String {
        self.read(|session| yes_no(session.app.running()))
    }

    #[must_use]
    pub fn marks(&self) -> String {
        self.read(|session| {
            let marks = if session.app.view() == cyclops_tui::state::View::Diagram {
                session.app.diagram_marks()
            } else {
                session.app.tree_marks()
            };
            marks
                .get(&SymbolId::new(&self.subject))
                .cloned()
                .unwrap_or_else(|| NONE.into())
        })
    }

    /// The worst of the current metric, as the Overview row of it says.
    #[must_use]
    pub fn worst(&self) -> String {
        self.read(|session| {
            let Some(analysis) = session.app.analysis() else {
                return NONE.into();
            };
            analysis.worst(session.app.lens().metric).map_or_else(
                || NONE.into(),
                |worst| {
                    let names: Vec<String> =
                        worst.subjects.iter().map(ToString::to_string).collect();
                    list(&names)
                },
            )
        })
    }

    /// The value on the Overview row of the current metric.
    #[must_use]
    pub fn value(&self) -> String {
        self.read(|session| {
            let metric = session.app.lens().metric;
            session
                .app
                .analysis()
                .and_then(|analysis| analysis.worst(metric))
                .map_or_else(
                    || NOT_APPLICABLE.into(),
                    |worst| metric.descriptor().format(worst.value),
                )
        })
    }

    #[must_use]
    pub fn ranking(&self) -> String {
        self.read(|session| {
            let ranked: Vec<String> = session
                .app
                .ranking()
                .iter()
                .map(ToString::to_string)
                .collect();
            list(&ranked)
        })
    }

    #[must_use]
    pub fn mascot(&self) -> String {
        self.read(|session| session.app.pose().name().to_owned())
    }

    #[must_use]
    pub fn opened(&self) -> String {
        self.read(|session| session.app.opened().unwrap_or(NONE).to_owned())
    }

    #[must_use]
    pub fn clipboard(&self) -> String {
        self.read(|session| session.app.copied().unwrap_or(NONE).to_owned())
    }

    #[must_use]
    pub fn completions(&self) -> String {
        self.read(|session| {
            let names: Vec<String> = session
                .app
                .completions()
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            list(&names)
        })
    }

    #[must_use]
    pub fn sorted_by(&self) -> String {
        self.read(|session| {
            let (metric, lowest_first) = session.app.sorted_by();
            let order = if lowest_first { "asc" } else { "desc" };
            format!("{} {order}", metric.name())
        })
    }

    /// How many files the last analysis parsed.
    #[must_use]
    pub fn parsed(&self) -> String {
        self.read(|session| session.app.parsed_by_refresh().to_string())
    }

    #[must_use]
    pub fn breadcrumb(&self) -> String {
        self.read(|session| list(&session.app.breadcrumb()))
    }

    #[must_use]
    pub fn theme(&self) -> String {
        self.read(|session| session.app.theme().name().to_owned())
    }

    #[must_use]
    pub fn indexers_run(&self) -> String {
        self.read(|session| yes_no(session.scenario.indexers_ran()))
    }

    #[must_use]
    pub fn created(&self) -> String {
        self.read(|session| {
            let created: Vec<String> = crate::cli::files_in(session.scenario.root())
                .difference(&session.before)
                .cloned()
                .collect();
            list(&created)
        })
    }
}
