//! Writes what an analysis says the way the tables expect it.

use cyclops_application::analysis::ProjectAnalysis;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::{Symbol, SymbolId};

pub const NONE: &str = "none";
pub const NOT_APPLICABLE: &str = "n/a";

pub fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.into()
}

/// A list the way the tables write it.
pub fn list(items: &[String]) -> String {
    if items.is_empty() {
        NONE.into()
    } else {
        items.join(", ")
    }
}

/// A subject with an extension is a file, and anything else is a symbol.
pub fn subject(text: &str) -> Subject {
    let file_name = text.rsplit('/').next().unwrap_or(text);
    if file_name.contains('.') && !text.contains("::") {
        Subject::File(RelativePath::new(text))
    } else {
        Subject::Symbol(SymbolId::new(text))
    }
}

pub fn metric_value(analysis: &ProjectAnalysis, subject: &Subject, metric: MetricId) -> String {
    analysis.value(subject, metric).map_or_else(
        || NOT_APPLICABLE.into(),
        |value| metric.descriptor().format(value),
    )
}

/// A symbol, whether test code is visible or not.
pub fn symbol<'a>(analysis: &'a ProjectAnalysis, id: &SymbolId) -> Option<&'a Symbol> {
    analysis.model().symbol(id)
}
