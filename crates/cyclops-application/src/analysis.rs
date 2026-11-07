//! The result of an analysis, and what analyzers get to work with.

use cyclops_domain::facts::{FileFacts, LineCounts};
use cyclops_domain::language::LanguageId;
use cyclops_domain::metric::{Measurement, MetricId, Subject};
use cyclops_domain::model::{ModelView, ProjectModel, TestCode};
use cyclops_domain::path::RelativePath;
use cyclops_domain::problem::Problem;
use cyclops_domain::symbol::{CodeRole, Symbol, SymbolId};
use std::collections::BTreeMap;

/// A file that was analyzed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzedFile {
    pub path: RelativePath,
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
}

impl ProjectAnalysis {
    pub(crate) fn new(
        model: ProjectModel,
        files: Vec<AnalyzedFile>,
        measurements: Vec<Measurement>,
        problems: Vec<Problem>,
        tests: TestCode,
        files_parsed: usize,
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
        }
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
