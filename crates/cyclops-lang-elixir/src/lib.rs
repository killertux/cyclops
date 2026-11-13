//! The Elixir language adapter, built on tree-sitter. It only describes code, and never
//! measures it.

mod extract;
mod language;
mod link;
mod symbols;
mod tree;

use cyclops_application::ports::{FileSource, LanguageAdapter, Linked, SourceFile};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;

/// Raise it whenever the facts this adapter extracts change, so that cached ones from an
/// older version are parsed again.
const VERSION: u32 = 1;

#[derive(Debug, Default)]
pub struct ElixirAdapter;

impl LanguageAdapter for ElixirAdapter {
    fn name(&self) -> &'static str {
        "elixir"
    }

    fn version(&self) -> u32 {
        VERSION
    }

    fn language_of(&self, path: &RelativePath) -> Option<LanguageId> {
        language::of(path)
    }

    fn extract(&self, source: &SourceFile<'_>) -> FileFacts {
        extract::extract(source.path, source.text)
    }

    fn link(&self, facts: &[&FileFacts], source: &dyn FileSource) -> Linked {
        link::link(facts, source)
    }
}
