//! The TypeScript and JavaScript language adapter, built on oxc. It only describes code,
//! and never measures it.

mod extract;
mod language;
mod link;
mod symbols;

use cyclops_application::ports::{FileSource, LanguageAdapter, Linked, SourceFile};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::path::RelativePath;

#[derive(Debug, Default)]
pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn handles(&self, path: &RelativePath) -> bool {
        language::of(path).is_some()
    }

    fn extract(&self, source: &SourceFile<'_>) -> FileFacts {
        extract::extract(source.path, source.text)
    }

    fn link(&self, facts: &[&FileFacts], source: &dyn FileSource) -> Linked {
        link::link(facts, source)
    }
}
