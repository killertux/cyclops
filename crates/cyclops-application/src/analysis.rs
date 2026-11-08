//! The result of an analysis, and what analyzers get to work with.

use crate::duplication::Duplication;
use crate::history::CommitSummary;
use crate::ports::StepReport;
use crate::precise::PreciseStatus;
use crate::queries::Alert;
use cyclops_domain::alert::AlertRule;
use cyclops_domain::facts::{FileFacts, LineCounts};
use cyclops_domain::language::LanguageId;
use cyclops_domain::metric::{Measurement, MetricId, Subject};
use cyclops_domain::model::{ModelView, ProjectModel, TestCode};
use cyclops_domain::path::RelativePath;
use cyclops_domain::problem::Problem;
use cyclops_domain::settings::Severity;
use cyclops_domain::symbol::{CodeRole, Symbol, SymbolId};
use std::collections::BTreeMap;

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
    /// The alerts the rules fire.
    pub alerts: Vec<Alert>,
    pub precise: PreciseStatus,
    /// What each step did.
    pub steps: Vec<StepReport>,
    /// The alert rules, enabled or not.
    pub rules: Vec<AlertRule>,
    /// The severity at which `cyclops check` fails.
    pub fail_on: Severity,
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

    pub(crate) fn set_alerts(&mut self, alerts: Vec<Alert>) {
        self.extras.alerts = alerts;
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

    /// What the analysis wants you to know, in order of file.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// A value of a metric, if it was measured.
    #[must_use]
    pub fn value(&self, subject: &Subject, metric: MetricId) -> Option<f64> {
        self.measurements.get(&(subject.clone(), metric)).copied()
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
