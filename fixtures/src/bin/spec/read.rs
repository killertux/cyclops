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

/// A subject with an extension is a file, and anything else is a symbol. An Elixir module
/// such as `Shop.Order` has dots too, but each of its parts is capitalized, and an
/// extension is not.
pub fn subject(text: &str) -> Subject {
    let file_name = text.rsplit('/').next().unwrap_or(text);
    let has_extension = file_name
        .rsplit_once('.')
        .and_then(|(_, extension)| extension.chars().next())
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit());
    if has_extension && !text.contains("::") {
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

/// The groups of methods of a type, separated by semicolons, or the fields each uses.
pub fn method_groups(analysis: &ProjectAnalysis, id: &SymbolId, fields: bool) -> String {
    let Some(groups) = analysis.method_groups(id) else {
        return NONE.into();
    };
    let written: Vec<String> = groups
        .iter()
        .map(|group| {
            if fields {
                group.fields.join(", ")
            } else {
                group.methods.join(", ")
            }
        })
        .collect();
    if written.is_empty() {
        NONE.into()
    } else {
        written.join("; ")
    }
}
