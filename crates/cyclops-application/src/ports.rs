//! What the application needs from the outside world.

use crate::analysis::AnalysisInput;
use cyclops_domain::facts::FileFacts;
use cyclops_domain::language::LanguageId;
use cyclops_domain::metric::Measurement;
use cyclops_domain::path::RelativePath;
use cyclops_domain::relation::Relation;
use cyclops_domain::symbol::{Symbol, SymbolId};
use std::collections::HashMap;
use thiserror::Error;

/// A file to extract facts from.
#[derive(Debug, Clone, Copy)]
pub struct SourceFile<'a> {
    pub path: &'a RelativePath,
    pub text: &'a str,
}

/// What linking found out about a whole project.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Linked {
    pub symbols: Vec<Symbol>,
    pub relations: Vec<Relation>,
    /// Files that are test code, which only the rest of the project could tell.
    pub test_files: Vec<RelativePath>,
    /// The module or package each file is.
    pub file_modules: Vec<(RelativePath, SymbolId)>,
}

/// Knows one language, or a family of them. An adapter describes code and never
/// measures it.
pub trait LanguageAdapter: Send + Sync {
    /// The name its cached facts are kept under.
    fn name(&self) -> &'static str;

    /// Goes up whenever the facts it extracts change, so that old ones are not reused.
    fn version(&self) -> u32;

    /// The language of a file, if this adapter understands it.
    fn language_of(&self, path: &RelativePath) -> Option<LanguageId>;

    /// Extracts what one file says. A file that cannot be fully understood still gives
    /// the facts around the problem.
    fn extract(&self, source: &SourceFile<'_>) -> FileFacts;

    /// Places the facts of every file of this language in the project.
    fn link(&self, facts: &[&FileFacts], source: &dyn FileSource) -> Linked;
}

/// Turns the linked project into measurements.
pub trait Analyzer: Send + Sync {
    fn analyze(&self, input: &AnalysisInput<'_>) -> Vec<Measurement>;
}

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("the project cannot be listed: {0}")]
    Unlistable(String),
    #[error("{0} cannot be read")]
    Unreadable(RelativePath),
}

/// What a look through a project found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovered {
    /// Every file that is not ignored, in order of path.
    pub files: Vec<RelativePath>,
    /// The folders that could not be listed, in order of path.
    pub unreadable: Vec<RelativePath>,
}

/// What can be told about a file without reading it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    /// When it last changed, in nanoseconds since 1970.
    pub modified: u64,
}

/// The files of a project.
pub trait FileSource: Sync {
    /// The name of the folder of the project.
    fn project_name(&self) -> String;

    /// Looks through the project.
    ///
    /// # Errors
    ///
    /// Fails if the project cannot be listed at all.
    fn discover(&self) -> Result<Discovered, SourceError>;

    /// The size and the time of a file.
    ///
    /// # Errors
    ///
    /// Fails if the file does not exist or cannot be read.
    fn stamp(&self, path: &RelativePath) -> Result<FileStamp, SourceError>;

    /// The contents of a file.
    ///
    /// # Errors
    ///
    /// Fails if the file does not exist or cannot be read.
    fn read(&self, path: &RelativePath) -> Result<Vec<u8>, SourceError>;
}

/// A digest of a file's contents.
pub type ContentHash = [u8; 32];

/// The facts of a file as they were last extracted.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedFacts {
    pub adapter: String,
    pub version: u32,
    pub stamp: FileStamp,
    pub hash: ContentHash,
    pub facts: FileFacts,
}

/// Facts to keep for next time.
#[derive(Debug, Clone, Copy)]
pub struct CacheEntry<'a> {
    pub adapter: &'a str,
    pub version: u32,
    pub stamp: FileStamp,
    pub hash: ContentHash,
    pub facts: &'a FileFacts,
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct CacheError(pub String);

/// Keeps facts between analyses, so that only what changed is parsed again. It can be
/// thrown away at any time.
pub trait FactStore {
    /// Everything it holds.
    ///
    /// # Errors
    ///
    /// Fails if the cache cannot be read.
    fn load(&mut self) -> Result<HashMap<RelativePath, CachedFacts>, CacheError>;

    /// Keeps the facts that changed, and forgets the files that are gone.
    ///
    /// # Errors
    ///
    /// Fails if the cache cannot be written.
    fn save(&mut self, changed: &[CacheEntry<'_>], gone: &[RelativePath])
    -> Result<(), CacheError>;

    /// The history read so far.
    ///
    /// # Errors
    ///
    /// Fails if the cache cannot be read.
    fn load_history(&mut self) -> Result<KnownHistory, CacheError>;

    /// Adds new commits to the history, or replaces it when it was rewritten.
    ///
    /// # Errors
    ///
    /// Fails if the cache cannot be written.
    fn save_history(&mut self, history: &NewHistory) -> Result<(), CacheError>;

    /// What each indexer found the last time precise analysis ran.
    ///
    /// # Errors
    ///
    /// Fails if the cache cannot be read.
    fn load_indexes(&mut self) -> Result<Vec<StoredIndex>, CacheError>;

    /// Keeps what an indexer found, in place of what it found before.
    ///
    /// # Errors
    ///
    /// Fails if the cache cannot be written.
    fn save_index(&mut self, index: &StoredIndex) -> Result<(), CacheError>;
}

/// What an analysis can do about the cache.
pub enum CacheAccess<'a> {
    /// Use this one.
    Store(&'a mut dyn FactStore),
    /// There is none, and that is a problem worth reporting.
    Unavailable(CacheError),
    /// Do without, on purpose.
    None,
}

/// One commit, with the files it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    /// When it was made, in seconds since 1970.
    pub time: i64,
    /// The first line of its message.
    pub subject: String,
    pub changes: Vec<FileChange>,
}

/// A file a commit changed, relative to the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    /// What it was called before, if the commit renamed it.
    pub previous: Option<String>,
}

/// What the history has that the cache does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHistory {
    pub head: String,
    /// The commits that are new, newest first.
    pub commits: Vec<Commit>,
    /// Whether the history the cache knew is gone, so that every commit is new.
    pub rewritten: bool,
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("no git history")]
    NoHistory,
    #[error("git history cannot be read: {0}")]
    Unreadable(String),
}

/// The history of a project's files.
pub trait HistorySource {
    /// The commits since `known`, the head the cache saw last.
    ///
    /// # Errors
    ///
    /// Fails if there is no history, or it cannot be read.
    fn read_since(&self, known: Option<&str>) -> Result<NewHistory, HistoryError>;
}

/// The history the cache keeps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownHistory {
    pub head: Option<String>,
    /// Every commit it has read, newest first.
    pub commits: Vec<Commit>,
}

/// What an indexer found in a project: where each name in the code is defined, and
/// where it is used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticIndex {
    pub documents: Vec<IndexedDocument>,
}

/// What an indexer found in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedDocument {
    pub path: RelativePath,
    pub definitions: Vec<IndexedDefinition>,
    pub references: Vec<IndexedReference>,
}

/// Where a name is defined. Lines count from 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedDefinition {
    pub line: u32,
    /// The name as the code writes it, such as `total`.
    pub name: String,
    /// What the indexer calls it, the same in every file.
    pub key: String,
}

/// Where a name is used. Lines count from 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedReference {
    pub line: u32,
    pub key: String,
    /// The key of what it belongs to, such as the class of a constructor, for when the
    /// name itself is not a symbol of the project.
    pub owner: Option<String>,
}

#[derive(Debug, Error)]
pub enum IndexError {
    /// A tool the indexer needs is not installed, such as `Node`.
    #[error("{0} not found")]
    Missing(String),
    #[error("{0}")]
    Failed(String),
}

/// Runs a compiler's indexer over the whole project, to find out exactly what each name
/// refers to.
pub trait SemanticProvider: Send + Sync {
    /// The name precise analysis gives its languages, such as `TypeScript`.
    fn label(&self) -> &'static str;

    fn languages(&self) -> &[LanguageId];

    /// Whether the tools it needs are there.
    ///
    /// # Errors
    ///
    /// Says which tool is missing.
    fn availability(&self) -> Result<(), IndexError>;

    /// Indexes the project. It is slow, and may run the project's build scripts.
    ///
    /// # Errors
    ///
    /// Fails if a tool is missing, or the indexer fails.
    fn index(&self) -> Result<SemanticIndex, IndexError>;
}

/// What precise analysis found for some languages, kept for later analyses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredIndex {
    /// The label of the indexer that made it.
    pub label: String,
    pub languages: Vec<LanguageId>,
    /// Every file of its languages, as it was when indexed.
    pub files: Vec<(RelativePath, ContentHash)>,
    /// The files the indexer read.
    pub covered: Vec<RelativePath>,
    /// Which symbol uses which: the innermost symbol around each use, and what it uses.
    pub uses: Vec<(SymbolId, SymbolId)>,
}
