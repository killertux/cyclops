//! Precise analysis: runs each language's indexer as a separate process and reads the
//! SCIP index it writes. The commands are built in, never read from settings, so that a
//! repository's committed files cannot make Cyclops run anything else.

mod indexers;
mod reader;
mod tools;

pub use indexers::{PythonIndexer, RustIndexer, TypeScriptIndexer, reason_of};
pub use reader::read_index;
pub use tools::Tools;

use cyclops_application::ports::{Log, SemanticProvider};
use std::path::Path;
use std::sync::Arc;

/// Every indexer Cyclops has, for the project at `root`. They write their indexes into
/// `output`, find their tools on `tools`, and write what they ran to `log`.
#[must_use]
pub fn providers(
    root: &Path,
    output: &Path,
    tools: &Tools,
    log: &Arc<dyn Log>,
) -> Vec<Box<dyn SemanticProvider>> {
    vec![
        Box::new(RustIndexer::new(
            root,
            output,
            tools.clone(),
            Arc::clone(log),
        )),
        Box::new(TypeScriptIndexer::new(
            root,
            output,
            tools.clone(),
            Arc::clone(log),
        )),
        Box::new(PythonIndexer::new(
            root,
            output,
            tools.clone(),
            Arc::clone(log),
        )),
    ]
}
