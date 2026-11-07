//! JSON for tools. It is written by hand, as the shapes are small and fixed.

use cyclops_application::analysis::ProjectAnalysis;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::symbol::Symbol;
use std::collections::BTreeMap;

/// A JSON string, with what must be escaped escaped.
fn string(text: &str) -> String {
    let mut quoted = String::from('"');
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                quoted.push_str("\\u00");
                for digit in [u32::from(c) >> 4, u32::from(c) & 0xf] {
                    quoted.extend(char::from_digit(digit, 16));
                }
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

fn object(fields: &[(&str, String)]) -> String {
    let fields: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{}:{value}", string(name)))
        .collect();
    format!("{{{}}}", fields.join(","))
}

fn array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

fn metrics(values: &[(MetricId, f64)]) -> String {
    let fields: Vec<(&str, String)> = values
        .iter()
        .map(|(metric, value)| (metric.name(), metric.descriptor().format(*value)))
        .collect();
    object(&fields)
}

/// Every metric of every symbol and file, and the problems, as one JSON document.
#[must_use]
pub fn report_json(analysis: &ProjectAnalysis) -> String {
    let mut rows: BTreeMap<&Subject, Vec<(MetricId, f64)>> = BTreeMap::new();
    for (subject, metric, value) in analysis.measurements() {
        rows.entry(subject).or_default().push((metric, value));
    }
    let view = analysis.view();
    let mut symbols = Vec::new();
    let mut files = Vec::new();
    for (subject, values) in rows {
        match subject {
            Subject::Symbol(id) => {
                let kind = view
                    .symbol(id)
                    .map_or("unknown", |symbol| symbol.kind.name());
                symbols.push(object(&[
                    ("id", string(id.as_str())),
                    ("kind", string(kind)),
                    ("metrics", metrics(&values)),
                ]));
            }
            Subject::File(path) => {
                files.push(object(&[
                    ("path", string(path.as_str())),
                    ("metrics", metrics(&values)),
                ]));
            }
        }
    }
    let problems: Vec<String> = analysis
        .problems()
        .iter()
        .map(|problem| string(&problem.to_string()))
        .collect();
    object(&[
        ("symbols", array(&symbols)),
        ("files", array(&files)),
        ("problems", array(&problems)),
    ])
}

/// Symbols found by a query, as a JSON list.
#[must_use]
pub fn symbols_json(found: &[&Symbol]) -> String {
    let items: Vec<String> = found
        .iter()
        .map(|symbol| {
            object(&[
                ("id", string(symbol.id.as_str())),
                ("kind", string(symbol.kind.name())),
            ])
        })
        .collect();
    array(&items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        assert_eq!(string("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
    }
}
