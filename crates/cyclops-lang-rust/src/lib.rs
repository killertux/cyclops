//! The Rust language adapter, built on rust-analyzer's parser, which tolerates broken
//! code. It only describes code, and never measures it.

mod body;
mod extract;
mod layout;
mod lines;
mod link;

use cyclops_application::ports::{FileSource, LanguageAdapter, Linked, SourceFile};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use std::path::Path;

/// Raise it whenever the facts this adapter extracts change, so that cached ones from an
/// older version are parsed again.
const VERSION: u32 = 1;

#[derive(Debug, Default)]
pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn version(&self) -> u32 {
        VERSION
    }

    fn language_of(&self, path: &RelativePath) -> Option<LanguageId> {
        let extension = Path::new(path.as_str()).extension()?;
        (extension == "rs").then_some(LanguageId::Rust)
    }

    fn extract(&self, source: &SourceFile<'_>) -> FileFacts {
        extract::extract(source.path, source.text)
    }

    fn link(&self, facts: &[&FileFacts], source: &dyn FileSource) -> Linked {
        link::link(facts, source)
    }
}
