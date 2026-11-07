//! Precise analysis: runs each language's indexer as a separate process and reads the
//! SCIP index it writes. The commands are built in, never read from settings, so that a
//! repository's committed files cannot make Cyclops run anything else.

mod indexers;
mod reader;
mod tools;

pub use indexers::{RustIndexer, TypeScriptIndexer};
pub use reader::read_index;
pub use tools::Tools;

use cyclops_application::ports::SemanticProvider;
use std::path::Path;

/// Every indexer Cyclops has, for the project at `root`. They write their indexes into
/// `output`, and find their tools on `tools`.
#[must_use]
pub fn providers(root: &Path, output: &Path, tools: &Tools) -> Vec<Box<dyn SemanticProvider>> {
    vec![
        Box::new(RustIndexer::new(root, output, tools.clone())),
        Box::new(TypeScriptIndexer::new(root, output, tools.clone())),
    ]
}
