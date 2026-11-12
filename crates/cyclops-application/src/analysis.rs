//! The result of an analysis, and what analyzers get to work with.

use crate::duplication::Duplication;
use crate::graph::ModuleGraph;
use crate::history::CommitSummary;
use crate::ports::StepReport;
use crate::precise::PreciseStatus;
use crate::queries::{Alert, Peers};
use cyclops_domain::alert::AlertRule;
use cyclops_domain::facts::{FileFacts, LineCounts};
use cyclops_domain::language::LanguageId;
use cyclops_domain::metric::{Measurement, MetricId, Subject};
use cyclops_domain::model::{ModelView, ProjectModel, TestCode};
use cyclops_domain::path::RelativePath;
use cyclops_domain::problem::Problem;
use cyclops_domain::relation::Confidence;
use cyclops_domain::settings::{SemanticMode, Severity};
use cyclops_domain::symbol::{CodeRole, Symbol, SymbolId};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

/// A file that was analyzed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzedFile {
    pub path: RelativePath,
    /// The module or package the file is.
    pub module: Option<SymbolId>,
    pub language: LanguageId,
    pub role: CodeRole,
    pub lines: LineCounts,
    pub test_lines: LineCounts,
}

impl AnalyzedFile {
    #[must_use]
    pub fn from_facts(facts: &FileFacts) -> Self {
        Self {
            path: facts.path.clone(),
            module: None,
            language: facts.language,
            role: facts.role,
            lines: facts.lines,
            test_lines: facts.test_lines,
        }
    }

    /// Marks all of the file as test code.
    pub(crate) const fn make_test_code(&mut self) {
        self.role = CodeRole::Test;
        self.test_lines = self.lines;
    }

    /// Its lines that count, with or without test code.
    #[must_use]
    pub const fn counted_lines(&self, tests: TestCode) -> LineCounts {
        match tests {
            TestCode::Included => self.lines,
            TestCode::Excluded => self.lines.without(self.test_lines),
        }
    }
}

/// What an analyzer works from.
#[derive(Debug, Clone, Copy)]
pub struct AnalysisInput<'a> {
    pub model: ModelView<'a>,
    pub files: &'a [AnalyzedFile],
    pub tests: TestCode,
}

impl AnalysisInput<'_> {
    /// The files that count, with or without test code.
    pub fn counted_files(&self) -> impl Iterator<Item = &AnalyzedFile> {
        self.files.iter().filter(|file| self.tests.shows(file.role))
    }
}

/// A finished analysis.
#[derive(Debug, Clone)]
pub struct ProjectAnalysis {
    model: ProjectModel,
    files: Vec<AnalyzedFile>,
    measurements: BTreeMap<(Subject, MetricId), f64>,
    problems: Vec<Problem>,
    tests: TestCode,
    files_parsed: usize,
    extras: Extras,
    /// The values of each symbol, to look up without building a subject.
    symbol_values: HashMap<SymbolId, Vec<(MetricId, f64)>>,
    memo: Memo,
}

/// Subjects with their values, the worst first.
pub(crate) type Ranked = Vec<(Subject, f64)>;

/// What queries work out once and keep, since the model and the measurements never
/// change after the analysis is made. Each part is worked out the first time it is asked
/// for.
#[derive(Debug, Clone, Default)]
pub(crate) struct Memo {
    /// Every peer with a value for a metric, the worst first.
    pub ranked: HashMap<(Peers, MetricId), OnceLock<Ranked>>,
    /// Which modules each module imports.
    pub graph: OnceLock<ModuleGraph>,
    /// The groups of modules that depend on each other, in order.
    pub cycles: OnceLock<Vec<Vec<SymbolId>>>,
    /// The visible symbols at the top of the hierarchy, in order of id.
    pub roots: OnceLock<Vec<SymbolId>>,
    /// How many relations between visible symbols are exact, inferred and ambiguous.
    pub confidences: OnceLock<[usize; 3]>,
}

impl Memo {
    fn new() -> Self {
        let ranked = Peers::ALL
            .iter()
            .flat_map(|peers| {
                MetricId::ALL
                    .iter()
                    .map(|metric| ((*peers, *metric), OnceLock::new()))
            })
            .collect();
        Self {
            ranked,
            ..Self::default()
        }
    }
}

/// Metrics that could not be worked out, and why. It reads as `churn, hotspot: no git
/// history`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub metrics: Vec<MetricId>,
    pub reason: String,
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.metrics.iter().map(|metric| metric.name()).collect();
        write!(f, "{}: {}", names.join(", "), self.reason)
    }
}

/// What an analysis found besides measurements.
#[derive(Debug, Clone, Default)]
pub struct Extras {
    pub duplication: Duplication,
    /// The metrics that could not be worked out, and why.
    pub unavailable: Vec<Unavailable>,
    /// How many commits this analysis read, rather than taking from the cache.
    pub commits_read: usize,
    /// The commits that changed each file within the hotspot window, newest first.
    pub file_commits: BTreeMap<RelativePath, Vec<CommitSummary>>,
    /// When the history was read, in seconds since 1970: the ages of commits count back
    /// from it.
    pub analyzed_at: i64,
    /// The alerts the rules fire.
    pub alerts: Vec<Alert>,
    pub precise: PreciseStatus,
    /// What each step did.
    pub steps: Vec<StepReport>,
    /// The alert rules, enabled or not.
    pub rules: Vec<AlertRule>,
    /// The severity at which `cyclops check` fails.
    pub fail_on: Severity,
    /// When precise analysis runs.
    pub semantic_mode: SemanticMode,
}

impl ProjectAnalysis {
    pub(crate) fn new(
        model: ProjectModel,
        files: Vec<AnalyzedFile>,
        measurements: Vec<Measurement>,
        problems: Vec<Problem>,
        tests: TestCode,
        files_parsed: usize,
        extras: Extras,
    ) -> Self {
        let mut symbol_values: HashMap<SymbolId, Vec<(MetricId, f64)>> = HashMap::new();
        for one in &measurements {
            // A value measured twice keeps the last, as the measurements do.
            if let Subject::Symbol(id) = &one.subject {
                let values = symbol_values.entry(id.clone()).or_default();
                match values.iter_mut().find(|(metric, _)| *metric == one.metric) {
                    Some((_, value)) => *value = one.value,
                    None => values.push((one.metric, one.value)),
                }
            }
        }
        let measurements = measurements
            .into_iter()
            .map(|one| ((one.subject, one.metric), one.value))
            .collect();
        Self {
            model,
            files,
            measurements,
            problems,
            tests,
            files_parsed,
            extras,
            symbol_values,
            memo: Memo::new(),
        }
    }

    /// The copies the project has.
    #[must_use]
    pub const fn duplication(&self) -> &Duplication {
        &self.extras.duplication
    }

    /// The metrics that could not be worked out, and why.
    #[must_use]
    pub fn unavailable(&self) -> &[Unavailable] {
        &self.extras.unavailable
    }

    /// Why a metric could not be worked out, if it could not.
    #[must_use]
    pub fn why_unavailable(&self, metric: MetricId) -> Option<&str> {
        self.extras
            .unavailable
            .iter()
            .find(|unavailable| unavailable.metrics.contains(&metric))
            .map(|unavailable| unavailable.reason.as_str())
    }

    /// How many commits this analysis read, rather than taking from the cache.
    #[must_use]
    pub const fn commits_read(&self) -> usize {
        self.extras.commits_read
    }

    /// The commits that changed a file within the hotspot window, newest first.
    #[must_use]
    pub fn commits_of(&self, file: &RelativePath) -> &[CommitSummary] {
        self.extras
            .file_commits
            .get(file)
            .map_or(&[], Vec::as_slice)
    }

    /// How long before the analysis a commit was made, in seconds.
    #[must_use]
    pub const fn age_of(&self, commit: &CommitSummary) -> i64 {
        self.extras.analyzed_at.saturating_sub(commit.time)
    }

    /// How many commits changed a file in each of the last twelve months of 30 days, the
    /// oldest month first.
    #[must_use]
    pub fn monthly_commits(&self, file: &RelativePath) -> [usize; 12] {
        const MONTH: i64 = 30 * 24 * 60 * 60;
        let mut months = [0; 12];
        for commit in self.commits_of(file) {
            // A commit after the analysis, or before the twelve months, falls outside.
            let Ok(back) = usize::try_from(self.age_of(commit) / MONTH) else {
                continue;
            };
            if let Some(month) = 11_usize
                .checked_sub(back)
                .and_then(|index| months.get_mut(index))
            {
                *month += 1;
            }
        }
        months
    }

    /// Which indexers ran, and whether the indexes are current.
    #[must_use]
    pub fn precise(&self) -> &PreciseStatus {
        &self.extras.precise
    }

    /// The alerts that fired, in order of severity and then subject.
    #[must_use]
    pub fn alerts(&self) -> &[Alert] {
        &self.extras.alerts
    }

    pub(crate) fn set_rules(&mut self, rules: Vec<AlertRule>) {
        self.extras.rules = rules;
    }

    /// The severity at which `cyclops check` fails.
    #[must_use]
    pub const fn fail_on(&self) -> Severity {
        self.extras.fail_on
    }

    /// When precise analysis runs, by the settings.
    #[must_use]
    pub const fn semantic_mode(&self) -> SemanticMode {
        self.extras.semantic_mode
    }

    pub(crate) const fn set_semantic_mode(&mut self, mode: SemanticMode) {
        self.extras.semantic_mode = mode;
    }

    pub(crate) const fn set_fail_on(&mut self, fail_on: Severity) {
        self.extras.fail_on = fail_on;
    }

    /// The alert rules, enabled or not, in the order of the settings.
    #[must_use]
    pub fn rules(&self) -> &[AlertRule] {
        &self.extras.rules
    }

    pub(crate) fn set_steps(&mut self, steps: Vec<StepReport>) {
        self.extras.steps = steps;
    }

    /// What each step of the analysis did, in order.
    #[must_use]
    pub fn steps(&self) -> &[StepReport] {
        &self.extras.steps
    }

    /// Adds a problem found once the analysis was made, in its place among the others.
    pub(crate) fn add_problem(&mut self, problem: Problem) {
        self.problems.push(problem);
        Problem::sort(&mut self.problems);
    }

    pub(crate) fn set_alerts(&mut self, alerts: Vec<Alert>) {
        self.extras.alerts = alerts;
    }

    /// An analysis made from its parts, for tests that need a project the pipeline
    /// cannot make, such as the made-up project the design draws.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn from_parts(
        model: ProjectModel,
        files: Vec<AnalyzedFile>,
        measurements: Vec<Measurement>,
        problems: Vec<Problem>,
        tests: TestCode,
        files_parsed: usize,
        extras: Extras,
    ) -> Self {
        Self::new(
            model,
            files,
            measurements,
            problems,
            tests,
            files_parsed,
            extras,
        )
    }

    /// Whether test code counts in this analysis.
    #[must_use]
    pub const fn tests(&self) -> TestCode {
        self.tests
    }

    /// How many files this analysis parsed, rather than taking from the cache.
    #[must_use]
    pub const fn files_parsed(&self) -> usize {
        self.files_parsed
    }

    /// The symbols that are visible, which leaves out test code when it is excluded.
    #[must_use]
    pub const fn view(&self) -> ModelView<'_> {
        ModelView::new(&self.model, self.tests)
    }

    /// Every symbol, whether it is visible or not.
    #[must_use]
    pub const fn model(&self) -> &ProjectModel {
        &self.model
    }

    /// Every file that was analyzed, in order of path, whether it counts or not.
    #[must_use]
    pub fn files(&self) -> &[AnalyzedFile] {
        &self.files
    }

    /// How many files count, and their lines of code: test code is left out unless it
    /// counts.
    #[must_use]
    pub fn counted_size(&self) -> (usize, u32) {
        let counted = self.files.iter().filter(|file| self.tests.shows(file.role));
        counted.fold((0, 0), |(files, lines), file| {
            (files + 1, lines + file.counted_lines(self.tests).code)
        })
    }

    /// What the analysis wants you to know, in order of file.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// A value of a metric, if it was measured.
    #[must_use]
    pub fn value(&self, subject: &Subject, metric: MetricId) -> Option<f64> {
        match subject {
            Subject::Symbol(id) => self.symbol_value(id, metric),
            Subject::File(_) => self.measurements.get(&(subject.clone(), metric)).copied(),
        }
    }

    /// A value of a metric for a symbol, if it was measured.
    #[must_use]
    pub fn symbol_value(&self, id: &SymbolId, metric: MetricId) -> Option<f64> {
        self.symbol_values
            .get(id)?
            .iter()
            .find(|(measured, _)| *measured == metric)
            .map(|(_, value)| *value)
    }

    pub(crate) const fn memo(&self) -> &Memo {
        &self.memo
    }

    /// How many relations between visible symbols are of each confidence: exact,
    /// inferred, then ambiguous.
    #[must_use]
    pub fn relations_by_confidence(&self) -> [(Confidence, usize); 3] {
        let [exact, inferred, ambiguous] = *self.memo.confidences.get_or_init(|| {
            let mut counts = [0; 3];
            for relation in self.view().relations() {
                let at = match relation.confidence {
                    Confidence::Exact => 0,
                    Confidence::Inferred => 1,
                    Confidence::Ambiguous => 2,
                };
                if let Some(count) = counts.get_mut(at) {
                    *count += 1;
                }
            }
            counts
        });
        [
            (Confidence::Exact, exact),
            (Confidence::Inferred, inferred),
            (Confidence::Ambiguous, ambiguous),
        ]
    }

    /// Every measurement, in order of subject and then metric.
    pub fn measurements(&self) -> impl Iterator<Item = (&Subject, MetricId, f64)> {
        self.measurements
            .iter()
            .map(|((subject, metric), value)| (subject, *metric, *value))
    }

    /// The children of a symbol, in order, as far as they are visible.
    #[must_use]
    pub fn children(&self, id: &SymbolId) -> Vec<&Symbol> {
        self.view().children(id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, role: CodeRole, code: u32, test_code: u32) -> AnalyzedFile {
        AnalyzedFile {
            path: RelativePath::new(path),
            module: None,
            language: LanguageId::Rust,
            role,
            lines: LineCounts {
                code,
                comment: 0,
                blank: 0,
            },
            test_lines: LineCounts {
                code: test_code,
                comment: 0,
                blank: 0,
            },
        }
    }

    fn with(files: Vec<AnalyzedFile>, tests: TestCode, extras: Extras) -> ProjectAnalysis {
        ProjectAnalysis::new(
            ProjectModel::new(Vec::new(), Vec::new()),
            files,
            Vec::new(),
            Vec::new(),
            tests,
            0,
            extras,
        )
    }

    #[test]
    fn test_code_counts_only_when_it_is_included() {
        let files = || {
            vec![
                file("src/a.rs", CodeRole::Production, 100, 20),
                file("tests/b.rs", CodeRole::Test, 50, 50),
            ]
        };
        let left_out = with(files(), TestCode::Excluded, Extras::default());
        assert_eq!(left_out.counted_size(), (1, 80));
        let counted = with(files(), TestCode::Included, Extras::default());
        assert_eq!(counted.counted_size(), (2, 150));
    }

    #[test]
    fn commits_count_in_the_month_they_were_made() {
        const DAY: i64 = 24 * 60 * 60;
        let now = 1_000 * DAY;
        let commit = |days: i64| CommitSummary {
            short_id: "abcdef0".to_owned(),
            time: now - days * DAY,
            subject: "Change".to_owned(),
        };
        let path = RelativePath::new("src/a.rs");
        let extras = Extras {
            analyzed_at: now,
            file_commits: [(
                path.clone(),
                vec![commit(1), commit(2), commit(40), commit(400)],
            )]
            .into_iter()
            .collect(),
            ..Extras::default()
        };
        let project = with(Vec::new(), TestCode::Excluded, extras);
        let months = project.monthly_commits(&path);
        // The newest month is last, and a commit older than a year is left out.
        assert_eq!(months[11], 2);
        assert_eq!(months[10], 1);
        assert_eq!(months.iter().sum::<usize>(), 3);
    }
}
