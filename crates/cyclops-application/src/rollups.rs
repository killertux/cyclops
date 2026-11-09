//! Rollups: a metric of small symbols, shown on the larger symbols around them. See
//! `specs/metrics/rollups.md`.

use crate::analysis::AnalysisInput;
use cyclops_domain::metric::{Measurement, MetricId, Rollup, Subject};
use cyclops_domain::symbol::{SymbolId, SymbolKind};
use std::collections::{BTreeMap, HashSet};

fn is_container(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Module | SymbolKind::Package | SymbolKind::Project
    )
}

/// The rolled up values, as they build up.
#[derive(Default)]
struct Rolled(BTreeMap<(Subject, MetricId), f64>);

impl Rolled {
    fn put(&mut self, subject: Subject, metric: MetricId, value: f64, rollup: Rollup) {
        let slot = self.0.entry((subject, metric)).or_insert(match rollup {
            Rollup::Max => f64::MIN,
            Rollup::Sum | Rollup::None => 0.0,
        });
        *slot = match rollup {
            Rollup::Max => slot.max(value),
            Rollup::Sum | Rollup::None => *slot + value,
        };
    }

    /// The highest of the parts, for every type, module and package around a function.
    /// A function inside another is measured on its own, and does not roll up into it.
    fn highest(&mut self, input: &AnalysisInput<'_>, measured: &[Measurement]) {
        let view = &input.model;
        for measurement in measured {
            let rollup = measurement.metric.descriptor().rollup;
            let Subject::Symbol(id) = &measurement.subject else {
                continue;
            };
            if rollup != Rollup::Max {
                continue;
            }
            let mut current = view.symbol(id).and_then(|symbol| symbol.parent.clone());
            while let Some(parent) = current.take() {
                let Some(symbol) = view.symbol(&parent) else {
                    break;
                };
                if symbol.kind.is_type() || is_container(symbol.kind) {
                    let subject = Subject::Symbol(parent.clone());
                    self.put(subject, measurement.metric, measurement.value, rollup);
                }
                current.clone_from(&symbol.parent);
            }
        }
    }

    /// A file's cyclomatic complexity is the sum of its functions.
    fn file_complexity(&mut self, input: &AnalysisInput<'_>, measured: &[Measurement]) {
        for measurement in measured
            .iter()
            .filter(|one| one.metric == MetricId::Cyclomatic)
        {
            let Subject::Symbol(id) = &measurement.subject else {
                continue;
            };
            let file = input
                .model
                .symbol(id)
                .and_then(|symbol| symbol.location.as_ref())
                .map(|location| location.file.clone());
            if let Some(file) = file {
                self.put(
                    Subject::File(file),
                    MetricId::Cyclomatic,
                    measurement.value,
                    Rollup::Sum,
                );
            }
        }
    }

    /// Lines are the sum of the files a module or package is made of. A module written
    /// inside a file has the lines of what it spans.
    fn lines(&mut self, input: &AnalysisInput<'_>) {
        let view = &input.model;
        for file in input.counted_files() {
            let lines = file.counted_lines(input.tests);
            let mut current = file.module.clone();
            while let Some(id) = current.take() {
                let Some(symbol) = view.symbol(&id) else {
                    break;
                };
                if is_container(symbol.kind) {
                    for (metric, value) in [
                        (MetricId::Lines, lines.code),
                        (MetricId::CommentLines, lines.comment),
                        (MetricId::BlankLines, lines.blank),
                    ] {
                        self.put(
                            Subject::Symbol(id.clone()),
                            metric,
                            f64::from(value),
                            Rollup::Sum,
                        );
                    }
                }
                current.clone_from(&symbol.parent);
            }
        }
        let with_files: HashSet<&SymbolId> = input
            .files
            .iter()
            .filter_map(|file| file.module.as_ref())
            .collect();
        for symbol in view.symbols() {
            let inline = symbol.kind == SymbolKind::Module
                && !with_files.contains(&symbol.id)
                && symbol.location.is_some();
            if inline {
                let subject = Subject::Symbol(symbol.id.clone());
                self.put(
                    subject,
                    MetricId::Lines,
                    f64::from(symbol.code_lines),
                    Rollup::Sum,
                );
            }
        }
    }
}

/// The values the larger symbols get from the measurements of the smaller ones.
#[must_use]
pub fn roll_up(input: &AnalysisInput<'_>, measured: &[Measurement]) -> Vec<Measurement> {
    let mut rolled = Rolled::default();
    rolled.highest(input, measured);
    rolled.file_complexity(input, measured);
    rolled.lines(input);
    rolled
        .0
        .into_iter()
        .map(|((subject, metric), value)| Measurement {
            metric,
            subject,
            value,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::AnalyzedFile;
    use cyclops_domain::facts::{BodyFacts, LineCounts};
    use cyclops_domain::language::LanguageId;
    use cyclops_domain::model::{ModelView, ProjectModel, TestCode};
    use cyclops_domain::path::RelativePath;
    use cyclops_domain::symbol::{CodeRole, Location, Span, Symbol};

    fn symbol(id: &str, parent: Option<&str>, kind: SymbolKind) -> Symbol {
        let mut symbol = Symbol::container(
            SymbolId::new(id),
            parent.map(SymbolId::new),
            id,
            kind,
            CodeRole::Production,
        );
        if matches!(kind, SymbolKind::Function | SymbolKind::Method) {
            symbol.body = Some(BodyFacts::default());
            symbol.location = Some(Location {
                file: RelativePath::new("a.rs"),
                span: Span {
                    start_line: 1,
                    end_line: 1,
                },
            });
        }
        symbol
    }

    fn cyclomatic(id: &str, value: f64) -> Measurement {
        Measurement {
            metric: MetricId::Cyclomatic,
            subject: Subject::Symbol(SymbolId::new(id)),
            value,
        }
    }

    fn value(rolled: &[Measurement], subject: &Subject, metric: MetricId) -> Option<f64> {
        rolled
            .iter()
            .find(|one| &one.subject == subject && one.metric == metric)
            .map(|one| one.value)
    }

    #[test]
    fn the_highest_goes_up_to_types_and_modules_but_not_into_other_functions() {
        let model = ProjectModel::new(
            vec![
                symbol("p", None, SymbolKind::Package),
                symbol("p::T", Some("p"), SymbolKind::Struct),
                symbol("p::T::f", Some("p::T"), SymbolKind::Method),
                symbol("p::T::f::g", Some("p::T::f"), SymbolKind::Function),
            ],
            Vec::new(),
        );
        let files = [AnalyzedFile {
            path: RelativePath::new("a.rs"),
            module: Some(SymbolId::new("p")),
            language: LanguageId::Rust,
            role: CodeRole::Production,
            lines: LineCounts {
                code: 7,
                comment: 1,
                blank: 2,
            },
            test_lines: LineCounts::default(),
        }];
        let input = AnalysisInput {
            model: ModelView::new(&model, TestCode::Excluded),
            files: &files,
            tests: TestCode::Excluded,
        };
        let measured = [cyclomatic("p::T::f", 2.0), cyclomatic("p::T::f::g", 5.0)];
        let rolled = roll_up(&input, &measured);

        let of = |id: &str| {
            value(
                &rolled,
                &Subject::Symbol(SymbolId::new(id)),
                MetricId::Cyclomatic,
            )
        };
        assert_eq!(of("p::T"), Some(5.0));
        assert_eq!(of("p"), Some(5.0));
        assert_eq!(of("p::T::f"), None, "a function keeps its own value");
        let file = Subject::File(RelativePath::new("a.rs"));
        assert_eq!(value(&rolled, &file, MetricId::Cyclomatic), Some(7.0));
        let package = Subject::Symbol(SymbolId::new("p"));
        assert_eq!(value(&rolled, &package, MetricId::Lines), Some(7.0));
        assert_eq!(value(&rolled, &package, MetricId::BlankLines), Some(2.0));
    }
}
