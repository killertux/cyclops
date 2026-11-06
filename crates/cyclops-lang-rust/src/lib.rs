//! The Rust language adapter, built on rust-analyzer's parser, which tolerates broken
//! code. It only describes code, and never measures it.

mod body;
mod extract;
mod layout;
mod lines;
mod link;

use cyclops_application::ports::{FileSource, LanguageAdapter, SourceFile};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::Symbol;
use std::path::Path;

#[derive(Debug, Default)]
pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn handles(&self, path: &RelativePath) -> bool {
        Path::new(path.as_str())
            .extension()
            .is_some_and(|extension| extension == "rs")
    }

    fn extract(&self, source: &SourceFile<'_>) -> FileFacts {
        extract::extract(source.path, source.text)
    }

    fn link(&self, facts: &[&FileFacts], source: &dyn FileSource) -> Vec<Symbol> {
        link::link(facts, source)
    }
}
