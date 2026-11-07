//! The analysis pipeline: discover, check the cache, extract, link, analyze.

use crate::analysis::{AnalysisInput, AnalyzedFile, ProjectAnalysis};
use crate::ports::{
    Analyzer, CacheAccess, CacheEntry, CachedFacts, ContentHash, FactStore, FileSource, FileStamp,
    LanguageAdapter, SourceError, SourceFile,
};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::glob::Glob;
use cyclops_domain::language::LanguageId;
use cyclops_domain::model::{ModelView, ProjectModel};
use cyclops_domain::path::RelativePath;
use cyclops_domain::problem::Problem;
use cyclops_domain::settings::Settings;
use cyclops_domain::symbol::CodeRole;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error("the analysis cannot start its workers: {0}")]
    Workers(#[from] rayon::ThreadPoolBuildError),
}

/// Walking a syntax tree recurses, and a long chain such as `a + b + c` is a deep tree
/// however flat it reads. This only reserves address space, so it costs nothing unless
/// it is used.
const WORKER_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Runs analyses. It holds the adapters and analyzers the binary chose.
pub struct AnalysisService {
    adapters: Vec<Box<dyn LanguageAdapter>>,
    analyzers: Vec<Box<dyn Analyzer>>,
}

/// A file some adapter understands, and what the cache knows about it.
struct Candidate<'a> {
    index: usize,
    adapter: &'a dyn LanguageAdapter,
    path: RelativePath,
    cached: Option<CachedFacts>,
}

/// Where a file's facts came from.
enum Origin {
    /// Unchanged since the cache saw it.
    Cache,
    /// The same contents with a new time, so the cache needs the new time.
    Touched(FileStamp, ContentHash),
    Parsed(FileStamp, ContentHash),
}

/// The facts of one file, and which adapter they came from.
struct Extracted {
    adapter: usize,
    facts: FileFacts,
    origin: Origin,
}

fn hash(bytes: &[u8]) -> ContentHash {
    *blake3::hash(bytes).as_bytes()
}

/// Settings that mark more files as test code apply after the cache, so that changing
/// them never parses anything again.
fn mark_test_paths(facts: &mut FileFacts, test_paths: &[Glob]) {
    let path = facts.path.as_str();
    if test_paths.iter().any(|pattern| pattern.matches(path)) {
        facts.role = CodeRole::Test;
        facts.test_lines = facts.lines;
        for symbol in &mut facts.symbols {
            symbol.role = CodeRole::Test;
        }
    }
}

impl AnalysisService {
    #[must_use]
    pub fn new(adapters: Vec<Box<dyn LanguageAdapter>>, analyzers: Vec<Box<dyn Analyzer>>) -> Self {
        Self {
            adapters,
            analyzers,
        }
    }

    /// The languages a project has, among those Cyclops knows.
    ///
    /// # Errors
    ///
    /// Fails if the project cannot be listed at all.
    pub fn languages_in(&self, source: &dyn FileSource) -> Result<Vec<LanguageId>, AnalysisError> {
        let mut languages: Vec<LanguageId> = source
            .discover()?
            .files
            .iter()
            .filter_map(|path| {
                self.adapters
                    .iter()
                    .find_map(|adapter| adapter.language_of(path))
            })
            .collect();
        languages.sort();
        languages.dedup();
        Ok(languages)
    }

    /// Analyzes a project. One bad file never stops it, and only shows up as a problem.
    /// Without a cache, or with one that fails, every file is parsed.
    ///
    /// # Errors
    ///
    /// Fails if the project cannot be listed at all.
    pub fn analyze(
        &self,
        source: &dyn FileSource,
        cache: CacheAccess<'_>,
        settings: &Settings,
    ) -> Result<ProjectAnalysis, AnalysisError> {
        let mut problems = Vec::new();
        let mut cache = match cache {
            CacheAccess::Store(store) => Some(store),
            CacheAccess::Unavailable(error) => {
                problems.push(Problem::CacheUnavailable(error.to_string()));
                None
            }
            CacheAccess::None => None,
        };
        let mut known = match cache.as_mut().map(|store| store.load()) {
            Some(Ok(known)) => known,
            Some(Err(error)) => {
                problems.push(Problem::CacheUnavailable(error.to_string()));
                cache = None;
                HashMap::new()
            }
            None => HashMap::new(),
        };

        let discovered = source.discover()?;
        let present: HashSet<&RelativePath> = discovered.files.iter().collect();
        let gone: Vec<RelativePath> = known
            .keys()
            .filter(|path| !present.contains(path))
            .cloned()
            .collect();
        let candidates = self.candidates(discovered.files.clone(), settings, &mut known);
        let found_any = !candidates.is_empty();
        problems.extend(discovered.unreadable.into_iter().map(Problem::Unreadable));

        let workers = ThreadPoolBuilder::new()
            .stack_size(WORKER_STACK_BYTES)
            .build()?;
        let outcomes: Vec<Result<Extracted, Problem>> = workers.install(|| {
            candidates
                .into_par_iter()
                .map(|candidate| Self::extract(candidate, source))
                .collect()
        });

        let mut extracted = Vec::new();
        for outcome in outcomes {
            match outcome {
                Ok(file) => {
                    if file.facts.syntax_error {
                        problems.push(Problem::SyntaxError(file.facts.path.clone()));
                    }
                    extracted.push(file);
                }
                Err(problem) => problems.push(problem),
            }
        }
        if !found_any && problems.is_empty() {
            problems.push(Problem::NoSourceFiles);
        }
        let parsed = extracted
            .iter()
            .filter(|file| matches!(file.origin, Origin::Parsed(..)))
            .count();
        if let Some(store) = cache
            && let Err(error) = self.remember(store, &extracted, &gone)
        {
            problems.push(Problem::CacheUnavailable(error.to_string()));
        }
        Problem::sort(&mut problems);

        for file in &mut extracted {
            mark_test_paths(&mut file.facts, &settings.test_paths);
        }
        let (model, test_files) = self.link(&extracted, source);
        let mut files: Vec<AnalyzedFile> = extracted
            .iter()
            .map(|file| AnalyzedFile::from_facts(&file.facts))
            .collect();
        for file in files
            .iter_mut()
            .filter(|file| test_files.contains(&file.path))
        {
            file.make_test_code();
        }
        let input = AnalysisInput {
            model: ModelView::new(&model, settings.tests),
            files: &files,
            tests: settings.tests,
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
            settings.tests,
            parsed,
        ))
    }

    /// The files to analyze, with the adapter for each and what the cache knows.
    fn candidates(
        &self,
        paths: Vec<RelativePath>,
        settings: &Settings,
        known: &mut HashMap<RelativePath, CachedFacts>,
    ) -> Vec<Candidate<'_>> {
        paths
            .into_iter()
            .filter(|path| {
                !settings
                    .exclude
                    .iter()
                    .any(|pattern| pattern.matches(path.as_str()))
            })
            .filter_map(|path| {
                let (index, adapter, language) =
                    self.adapters
                        .iter()
                        .enumerate()
                        .find_map(|(index, adapter)| {
                            let language = adapter.language_of(&path)?;
                            Some((index, adapter.as_ref(), language))
                        })?;
                if !settings.languages.contains(&language) {
                    return None;
                }
                let cached = known.remove(&path).filter(|cached| {
                    cached.adapter == adapter.name() && cached.version == adapter.version()
                });
                Some(Candidate {
                    index,
                    adapter,
                    path,
                    cached,
                })
            })
            .collect()
    }

    fn extract(candidate: Candidate<'_>, source: &dyn FileSource) -> Result<Extracted, Problem> {
        let path = &candidate.path;
        let unreadable = |_| Problem::Unreadable(path.clone());
        let stamp = source.stamp(path).map_err(unreadable)?;
        if let Some(cached) = candidate
            .cached
            .as_ref()
            .filter(|cached| cached.stamp == stamp)
        {
            return Ok(Extracted {
                adapter: candidate.index,
                facts: cached.facts.clone(),
                origin: Origin::Cache,
            });
        }
        let bytes = source.read(path).map_err(unreadable)?;
        let digest = hash(&bytes);
        if let Some(cached) = candidate.cached.filter(|cached| cached.hash == digest) {
            return Ok(Extracted {
                adapter: candidate.index,
                facts: cached.facts,
                origin: Origin::Touched(stamp, digest),
            });
        }
        let text = String::from_utf8(bytes).map_err(|_| Problem::NotUtf8(path.clone()))?;
        let facts = candidate.adapter.extract(&SourceFile { path, text: &text });
        Ok(Extracted {
            adapter: candidate.index,
            facts,
            origin: Origin::Parsed(stamp, digest),
        })
    }

    fn remember(
        &self,
        store: &mut dyn FactStore,
        extracted: &[Extracted],
        gone: &[RelativePath],
    ) -> Result<(), crate::ports::CacheError> {
        let changed: Vec<CacheEntry<'_>> = extracted
            .iter()
            .filter_map(|file| {
                let (stamp, hash) = match file.origin {
                    Origin::Cache => return None,
                    Origin::Touched(stamp, hash) | Origin::Parsed(stamp, hash) => (stamp, hash),
                };
                let adapter = self.adapters.get(file.adapter)?;
                Some(CacheEntry {
                    adapter: adapter.name(),
                    version: adapter.version(),
                    stamp,
                    hash,
                    facts: &file.facts,
                })
            })
            .collect();
        if changed.is_empty() && gone.is_empty() {
            return Ok(());
        }
        store.save(&changed, gone)
    }

    fn link(
        &self,
        extracted: &[Extracted],
        source: &dyn FileSource,
    ) -> (ProjectModel, HashSet<RelativePath>) {
        let mut symbols = Vec::new();
        let mut test_files = HashSet::new();
        for (index, adapter) in self.adapters.iter().enumerate() {
            let facts: Vec<&FileFacts> = extracted
                .iter()
                .filter(|file| file.adapter == index)
                .map(|file| &file.facts)
                .collect();
            if !facts.is_empty() {
                let linked = adapter.link(&facts, source);
                symbols.extend(linked.symbols);
                test_files.extend(linked.test_files);
            }
        }
        (ProjectModel::new(symbols), test_files)
    }
}
