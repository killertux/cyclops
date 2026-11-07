//! What the application needs from the outside world.

use crate::analysis::AnalysisInput;
use cyclops_domain::facts::FileFacts;
use cyclops_domain::metric::Measurement;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::Symbol;
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

/// Knows one language. An adapter describes code and never measures it.
pub trait LanguageAdapter: Send + Sync {
    /// Whether this adapter understands a file.
    fn handles(&self, path: &RelativePath) -> bool;

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

    /// The contents of a file.
    ///
    /// # Errors
    ///
    /// Fails if the file does not exist or cannot be read.
    fn read(&self, path: &RelativePath) -> Result<Vec<u8>, SourceError>;
}
