//! What the application needs from the outside world.

use crate::analysis::AnalysisInput;
use cyclops_domain::facts::FileFacts;
use cyclops_domain::language::LanguageId;
use cyclops_domain::metric::Measurement;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::Symbol;
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
    /// Files that are test code, which only the rest of the project could tell.
    pub test_files: Vec<RelativePath>,
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
