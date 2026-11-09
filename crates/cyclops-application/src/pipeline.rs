//! The analysis pipeline: discover, check the cache, extract, link, analyze.

use crate::analysis::{AnalysisInput, AnalyzedFile, Extras, ProjectAnalysis, Unavailable};
use crate::analyzers::ComplexityAnalyzer;
use crate::duplication::Duplication;
use crate::history::{self, churn, start_of};
use crate::ports::{
    Analyzer, CacheAccess, CacheEntry, CachedFacts, Commit, ContentHash, FactStore, FileSource,
    FileStamp, HistoryError, HistorySource, IndexCommit, KnownHistory, LanguageAdapter, Partial,
    PartialFunction, Progress, SourceError, SourceFile, Step, StepReport, StoredIndex,
};
use crate::precise::{self, CurrentFile, Precise};
use crate::project::add_project;
use crate::rollups::roll_up;
use cyclops_domain::alert::AlertRule;
use cyclops_domain::facts::FileFacts;
use cyclops_domain::glob::Glob;
use cyclops_domain::language::LanguageId;
use cyclops_domain::metric::{Measurement, MetricId, Subject};
use cyclops_domain::model::{ModelView, ProjectModel};
use cyclops_domain::path::RelativePath;
use cyclops_domain::problem::Problem;
use cyclops_domain::relation::{Confidence, Relation};
use cyclops_domain::settings::{Settings, SettingsError};
use cyclops_domain::symbol::{CodeRole, Symbol, SymbolId};
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error("the analysis cannot start its workers: {0}")]
    Workers(#[from] rayon::ThreadPoolBuildError),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    /// Whoever asked for the analysis stopped it.
    #[error("the analysis was stopped")]
    Stopped,
}

/// The steps of one analysis as they happen.
struct Run<'a> {
    progress: &'a dyn Progress,
    steps: Vec<StepReport>,
    started: Instant,
}

impl Run<'_> {
    /// Starts a step, unless whoever asked for the analysis stops it here.
    fn begin(&mut self, step: Step) -> Result<(), AnalysisError> {
        if !self.progress.step(step) {
            return Err(AnalysisError::Stopped);
        }
        self.started = Instant::now();
        self.steps.push(StepReport {
            step,
            millis: 0,
            result: String::new(),
        });
        Ok(())
    }

    /// Says what an earlier step did, once a later one knows.
    fn amend(&mut self, step: Step, result: String) {
        if let Some(report) = self.steps.iter_mut().find(|report| report.step == step) {
            report.result = result;
        }
    }

    /// Says what the step that started last did.
    fn end(&mut self, result: String) {
        if let Some(report) = self.steps.last_mut() {
            report.millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
            report.result = result;
        }
    }
}

/// The files as the analyzers see them, with what linking said about each.
fn analyzed_files(extracted: &[Extracted], linked: &LinkedFiles) -> Vec<AnalyzedFile> {
    let mut files: Vec<AnalyzedFile> = extracted
        .iter()
        .map(|file| AnalyzedFile::from_facts(&file.facts))
        .collect();
    for file in &mut files {
        if linked.test_files.contains(&file.path) {
            file.make_test_code();
        }
        file.module = linked.file_modules.get(&file.path).cloned();
    }
    files
}

/// A count with commas between thousands, such as `21,840`.
fn grouped(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The name of a symbol within its file, such as `Order::total`.
fn local_name(facts: &FileFacts, index: usize) -> String {
    let mut parts = Vec::new();
    let mut current = facts.symbols.get(index);
    while let Some(symbol) = current {
        parts.push(symbol.name.clone());
        if let Some(owner) = &symbol.owner {
            parts.extend(owner.iter().rev().cloned());
            break;
        }
        current = symbol.parent.and_then(|parent| facts.symbols.get(parent));
    }
    parts.reverse();
    parts.join("::")
}

/// What files alone say about their functions, before the project is linked.
fn partial_of(facts: &[&FileFacts]) -> Partial {
    let mut functions = Vec::new();
    for file in facts {
        for (index, symbol) in file.symbols.iter().enumerate() {
            let Some(body) = &symbol.body else {
                continue;
            };
            functions.push(PartialFunction {
                file: file.path.clone(),
                name: local_name(file, index),
                role: symbol.role,
                values: vec![
                    (
                        MetricId::Cyclomatic,
                        f64::from(ComplexityAnalyzer::cyclomatic(body)),
                    ),
                    (
                        MetricId::Cognitive,
                        f64::from(ComplexityAnalyzer::cognitive(body)),
                    ),
                    (MetricId::MaxNesting, f64::from(body.max_nesting)),
                    (MetricId::Lines, f64::from(symbol.code_lines)),
                    (MetricId::Parameters, f64::from(body.parameters)),
                ],
            });
        }
    }
    Partial {
        files: facts.len(),
        functions,
    }
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

/// Where an analysis reads from.
pub struct Sources<'a> {
    pub files: &'a dyn FileSource,
    pub cache: CacheAccess<'a>,
    /// The history of the files, if the project has one.
    pub history: Option<&'a dyn HistorySource>,
    pub precise: Precise<'a>,
    /// Who hears how it goes, and may stop it.
    pub progress: &'a dyn Progress,
}

/// The time now, in seconds since 1970.
fn now() -> i64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
}

/// Every commit there is, from the cache and from what is new. Says how many were new.
fn read_history(
    history: Option<&dyn HistorySource>,
    cache: &mut Option<&mut dyn FactStore>,
    problems: &mut Vec<Problem>,
) -> Result<(Vec<Commit>, usize), HistoryError> {
    let source = history.ok_or(HistoryError::NoHistory)?;
    let known = match cache.as_deref_mut().map(<dyn FactStore>::load_history) {
        Some(Ok(known)) => known,
        Some(Err(error)) => {
            problems.push(Problem::CacheUnavailable(error.to_string()));
            KnownHistory::default()
        }
        None => KnownHistory::default(),
    };
    let new = source.read_since(known.head.as_deref())?;
    if let Some(store) = cache.as_deref_mut()
        && let Err(error) = store.save_history(&new)
    {
        problems.push(Problem::CacheUnavailable(error.to_string()));
    }
    let read = new.commits.len();
    let mut commits = new.commits;
    if !new.rewritten {
        commits.extend(known.commits);
    }
    Ok((commits, read))
}

/// What the Enrich step did, as Status says it.
fn enriched(indexes: &[StoredIndex], relations: &[Relation]) -> String {
    if indexes.is_empty() {
        return "skipped: no index yet".to_owned();
    }
    let exact = relations
        .iter()
        .filter(|relation| relation.confidence == Confidence::Exact)
        .count();
    format!("{exact} of {} relations exact", relations.len())
}

/// Gives the indexes made by this analysis the commit they were made at, the head, and
/// keeps them so. Says the commit of the indexes.
fn stamp_indexes(
    indexes: &mut [StoredIndex],
    head: Option<&Commit>,
    cache: &mut Option<&mut dyn FactStore>,
    problems: &mut Vec<Problem>,
) -> Option<IndexCommit> {
    let Some(head) = head else {
        return indexes.iter().find_map(|index| index.commit.clone());
    };
    for index in indexes.iter_mut().filter(|index| index.commit.is_none()) {
        index.commit = Some(IndexCommit {
            id: head.id.chars().take(7).collect(),
            subject: head.subject.clone(),
        });
        if let Some(store) = cache.as_deref_mut()
            && let Err(error) = store.save_index(index)
        {
            problems.push(Problem::CacheUnavailable(error.to_string()));
        }
    }
    indexes.iter().find_map(|index| index.commit.clone())
}

/// What linking says about files.
#[derive(Default)]
struct LinkedFiles {
    test_files: HashSet<RelativePath>,
    file_modules: HashMap<RelativePath, SymbolId>,
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
    hash: ContentHash,
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
    /// Without a cache, or with one that fails, every file is parsed. Without history,
    /// the metrics that need it are unavailable, and the rest still works.
    ///
    /// # Errors
    ///
    /// Fails if the project cannot be listed at all.
    pub fn analyze(
        &self,
        sources: Sources<'_>,
        settings: &Settings,
    ) -> Result<ProjectAnalysis, AnalysisError> {
        let rules = AlertRule::all(&settings.alerts)?;
        let mut run = Run {
            progress: sources.progress,
            steps: Vec::new(),
            started: Instant::now(),
        };
        let mut problems = Vec::new();
        let mut cache = match sources.cache {
            CacheAccess::Store(store) => Some(store),
            CacheAccess::Unavailable(error) => {
                problems.push(Problem::CacheUnavailable(error.to_string()));
                None
            }
            CacheAccess::None => None,
        };
        let (mut extracted, parsed) =
            self.gather(sources.files, &mut cache, settings, &mut problems, &mut run)?;
        for file in &mut extracted {
            mark_test_paths(&mut file.facts, &settings.test_paths);
        }

        run.begin(Step::Link)?;
        let (symbols, relations, linked) = self.link(&extracted, sources.files);
        run.end(format!(
            "{} relations, {} symbols",
            grouped(relations.len()),
            grouped(symbols.len())
        ));

        run.begin(Step::Enrich)?;
        let current: Vec<CurrentFile<'_>> = extracted
            .iter()
            .map(|file| CurrentFile {
                path: &file.facts.path,
                language: file.facts.language,
                hash: file.hash,
            })
            .collect();
        let (mut indexes, mut precise_status) = precise::run(
            sources.precise,
            settings.semantic,
            &symbols,
            &current,
            &mut cache,
            &mut problems,
            run.progress,
        )
        .ok_or(AnalysisError::Stopped)?;
        let relations = precise::enrich(relations, &symbols, &indexes, &current);
        run.end(enriched(&indexes, &relations));
        let model = ProjectModel::new(symbols, relations);

        run.begin(Step::History)?;
        let history = read_history(sources.history, &mut cache, &mut problems);
        run.end(match &history {
            Ok((_, read)) => format!("{read} new commits"),
            Err(error) => error.to_string(),
        });
        if let Ok((commits, _)) = &history {
            precise_status.commit =
                stamp_indexes(&mut indexes, commits.first(), &mut cache, &mut problems);
        }

        run.begin(Step::Analyze)?;
        let files = analyzed_files(&extracted, &linked);
        let (measurements, mut extras) =
            self.measure(&model, &files, &extracted, settings, history);
        extras.precise = precise_status;
        run.end(if extras.unavailable.is_empty() {
            "all metrics".to_owned()
        } else {
            let left_out: Vec<&str> = extras
                .unavailable
                .iter()
                .flat_map(|unavailable| unavailable.metrics.iter().map(|metric| metric.name()))
                .collect();
            format!("all metrics but {}", left_out.join(", "))
        });
        Problem::sort(&mut problems);

        run.begin(Step::Evaluate)?;
        let mut analysis = ProjectAnalysis::new(
            model,
            files,
            measurements,
            problems,
            settings.tests,
            parsed,
            extras,
        );
        let alerts = analysis.fire(&rules);
        run.end(format!("{} alerts", alerts.len()));
        analysis.set_alerts(alerts);
        analysis.set_steps(run.steps);
        analysis.set_rules(rules);
        analysis.set_fail_on(settings.fail_on);
        analysis.set_semantic_mode(settings.semantic);
        Ok(analysis)
    }

    /// Runs the analyzers, the rollups, duplication and history over the linked project.
    fn measure(
        &self,
        model: &ProjectModel,
        files: &[AnalyzedFile],
        extracted: &[Extracted],
        settings: &Settings,
        history: Result<(Vec<Commit>, usize), HistoryError>,
    ) -> (Vec<Measurement>, Extras) {
        let input = AnalysisInput {
            model: ModelView::new(model, settings.tests),
            files,
            tests: settings.tests,
        };
        let mut measurements: Vec<_> = self
            .analyzers
            .par_iter()
            .flat_map(|analyzer| analyzer.analyze(&input))
            .collect();
        let rolled = roll_up(&input, &measurements);
        measurements.extend(rolled);

        let facts: Vec<&FileFacts> = extracted.iter().map(|file| &file.facts).collect();
        let duplication = Duplication::find(&input, &facts, settings.min_tokens);
        measurements.extend(duplication.measure(&input));
        let mut extras = Extras {
            duplication,
            ..Extras::default()
        };
        match history {
            Ok((commits, read)) => {
                extras.commits_read = read;
                let paths: Vec<&RelativePath> = files.iter().map(|file| &file.path).collect();
                extras.analyzed_at = now();
                let since = start_of(settings.hotspots_since, extras.analyzed_at);
                let churn = churn(&commits, &paths, since);
                extras.file_commits = history::file_commits(&commits, &paths, since);
                let complexity: BTreeMap<RelativePath, f64> = measurements
                    .iter()
                    .filter(|one| one.metric == MetricId::Cyclomatic)
                    .filter_map(|one| match &one.subject {
                        Subject::File(path) => Some((path.clone(), one.value)),
                        Subject::Symbol(_) => None,
                    })
                    .collect();
                measurements.extend(history::measure(&input, &churn, &complexity));
            }
            Err(error) => extras.unavailable.push(Unavailable {
                metrics: vec![MetricId::Churn, MetricId::Hotspot],
                reason: error.to_string(),
            }),
        }
        (measurements, extras)
    }

    /// Finds the files, and extracts their facts or takes them from the cache. Says how
    /// many it parsed.
    fn gather(
        &self,
        source: &dyn FileSource,
        cache: &mut Option<&mut dyn FactStore>,
        settings: &Settings,
        problems: &mut Vec<Problem>,
        run: &mut Run<'_>,
    ) -> Result<(Vec<Extracted>, usize), AnalysisError> {
        run.begin(Step::Discover)?;
        let mut known = match cache.as_mut().map(|store| store.load()) {
            Some(Ok(known)) => known,
            Some(Err(error)) => {
                problems.push(Problem::CacheUnavailable(error.to_string()));
                *cache = None;
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
        run.end(format!("{} files", discovered.files.len()));
        run.begin(Step::Check)?;
        let candidates = self.candidates(discovered.files.clone(), settings, &mut known);
        let found_any = !candidates.is_empty();
        problems.extend(discovered.unreadable.into_iter().map(Problem::Unreadable));
        run.end(String::new());
        run.begin(Step::Extract)?;
        let total = candidates.len();
        let done = AtomicUsize::new(0);
        let progress = run.progress;

        let workers = ThreadPoolBuilder::new()
            .stack_size(WORKER_STACK_BYTES)
            .build()?;
        let outcomes: Vec<Result<Extracted, Problem>> = workers.install(|| {
            candidates
                .into_par_iter()
                .map(|candidate| {
                    let outcome = Self::extract(candidate, source);
                    progress.files(done.fetch_add(1, Ordering::Relaxed) + 1, total);
                    outcome
                })
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
        if let Some(store) = cache.as_deref_mut()
            && let Err(error) = self.remember(store, &extracted, &gone)
        {
            problems.push(Problem::CacheUnavailable(error.to_string()));
        }
        run.end(format!(
            "{parsed} parsed, {} from cache",
            extracted.len() - parsed
        ));
        run.amend(Step::Check, format!("{parsed} changed"));
        let facts: Vec<&FileFacts> = extracted.iter().map(|file| &file.facts).collect();
        run.progress.partial(partial_of(&facts));
        Ok((extracted, parsed))
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
                hash: cached.hash,
            });
        }
        let bytes = source.read(path).map_err(unreadable)?;
        let digest = hash(&bytes);
        if let Some(cached) = candidate.cached.filter(|cached| cached.hash == digest) {
            return Ok(Extracted {
                adapter: candidate.index,
                facts: cached.facts,
                origin: Origin::Touched(stamp, digest),
                hash: digest,
            });
        }
        let text = String::from_utf8(bytes).map_err(|_| Problem::NotUtf8(path.clone()))?;
        let facts = candidate.adapter.extract(&SourceFile { path, text: &text });
        Ok(Extracted {
            adapter: candidate.index,
            facts,
            origin: Origin::Parsed(stamp, digest),
            hash: digest,
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
    ) -> (Vec<Symbol>, Vec<Relation>, LinkedFiles) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let mut files = LinkedFiles::default();
        for (index, adapter) in self.adapters.iter().enumerate() {
            let facts: Vec<&FileFacts> = extracted
                .iter()
                .filter(|file| file.adapter == index)
                .map(|file| &file.facts)
                .collect();
            if !facts.is_empty() {
                let linked = adapter.link(&facts, source);
                symbols.extend(linked.symbols);
                relations.extend(linked.relations);
                files.test_files.extend(linked.test_files);
                files.file_modules.extend(linked.file_modules);
            }
        }
        add_project(&mut symbols, &source.project_name());
        (symbols, relations, files)
    }
}
