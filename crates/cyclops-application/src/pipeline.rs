//! The analysis pipeline: discover, extract, link, analyze.

use crate::analysis::{AnalysisInput, AnalyzedFile, ProjectAnalysis};
use crate::ports::{Analyzer, FileSource, LanguageAdapter, SourceError, SourceFile};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::model::{ModelView, ProjectModel, TestCode};
use cyclops_domain::path::RelativePath;
use cyclops_domain::problem::Problem;
use rayon::prelude::*;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnalysisOptions {
    pub tests: TestCode,
}

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Source(#[from] SourceError),
}

/// Runs analyses. It holds the adapters and analyzers the binary chose.
pub struct AnalysisService {
    adapters: Vec<Box<dyn LanguageAdapter>>,
    analyzers: Vec<Box<dyn Analyzer>>,
}

/// A file some adapter understands.
struct Candidate<'a> {
    index: usize,
    adapter: &'a dyn LanguageAdapter,
    path: RelativePath,
}

/// The facts of one file, and which adapter they came from.
struct Extracted {
    adapter: usize,
    facts: FileFacts,
}

impl AnalysisService {
    #[must_use]
    pub fn new(adapters: Vec<Box<dyn LanguageAdapter>>, analyzers: Vec<Box<dyn Analyzer>>) -> Self {
        Self {
            adapters,
            analyzers,
        }
    }

    /// Analyzes a project. One bad file never stops it, and only shows up as a problem.
    ///
    /// # Errors
    ///
    /// Fails if the project cannot be listed at all.
    pub fn analyze(
        &self,
        source: &dyn FileSource,
        options: AnalysisOptions,
    ) -> Result<ProjectAnalysis, AnalysisError> {
        let discovered = source.discover()?;
        let sources = self.source_files(discovered.files);
        let found_any = !sources.is_empty();
        let outcomes: Vec<Result<Extracted, Problem>> = sources
            .par_iter()
            .map(|file| Self::extract(file, source))
            .collect();

        let mut problems: Vec<Problem> = discovered
            .unreadable
            .into_iter()
            .map(Problem::Unreadable)
            .collect();
        let mut extracted = Vec::new();
        for outcome in outcomes {
            match outcome {
                Ok(file) if file.facts.syntax_error => {
                    problems.push(Problem::SyntaxError(file.facts.path.clone()));
                    extracted.push(file);
                }
                Ok(file) => extracted.push(file),
                Err(problem) => problems.push(problem),
            }
        }
        if !found_any && problems.is_empty() {
            problems.push(Problem::NoSourceFiles);
        }
        Problem::sort(&mut problems);

        let model = self.link(&extracted, source);
        let files: Vec<AnalyzedFile> = extracted
            .iter()
            .map(|file| AnalyzedFile::from_facts(&file.facts))
            .collect();
        let input = AnalysisInput {
            model: ModelView::new(&model, options.tests),
            files: &files,
            tests: options.tests,
        };
        let measurements = self
            .analyzers
            .par_iter()
            .flat_map(|analyzer| analyzer.analyze(&input))
            .collect();

        Ok(ProjectAnalysis::new(
            model,
            files,
            measurements,
            problems,
            options.tests,
        ))
    }

    /// The files some adapter understands, with that adapter.
    fn source_files(&self, paths: Vec<RelativePath>) -> Vec<Candidate<'_>> {
        paths
            .into_iter()
            .filter_map(|path| {
                let (index, adapter) = self
                    .adapters
                    .iter()
                    .enumerate()
                    .find(|(_, adapter)| adapter.handles(&path))?;
                let adapter = adapter.as_ref();
                Some(Candidate {
                    index,
                    adapter,
                    path,
                })
            })
            .collect()
    }

    fn extract(file: &Candidate<'_>, source: &dyn FileSource) -> Result<Extracted, Problem> {
        let path = &file.path;
        let bytes = source
            .read(path)
            .map_err(|_| Problem::Unreadable(path.clone()))?;
        let text = String::from_utf8(bytes).map_err(|_| Problem::NotUtf8(path.clone()))?;
        let facts = file.adapter.extract(&SourceFile { path, text: &text });
        Ok(Extracted {
            adapter: file.index,
            facts,
        })
    }

    fn link(&self, extracted: &[Extracted], source: &dyn FileSource) -> ProjectModel {
        let symbols = self
            .adapters
            .iter()
            .enumerate()
            .flat_map(|(index, adapter)| {
                let facts: Vec<&FileFacts> = extracted
                    .iter()
                    .filter(|file| file.adapter == index)
                    .map(|file| &file.facts)
                    .collect();
                if facts.is_empty() {
                    Vec::new()
                } else {
                    adapter.link(&facts, source)
                }
            })
            .collect();
        ProjectModel::new(symbols)
    }
}
