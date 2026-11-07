//! The analyzers. Each metric is written once, against language-neutral facts.

use crate::analysis::AnalysisInput;
use crate::ports::Analyzer;
use cyclops_domain::facts::BodyFacts;
use cyclops_domain::metric::{Measurement, MetricId, Subject};
use cyclops_domain::symbol::{Symbol, SymbolKind};

fn measure(symbol: &Symbol, metric: MetricId, value: u32) -> Measurement {
    Measurement {
        metric,
        subject: Subject::Symbol(symbol.id.clone()),
        value: f64::from(value),
    }
}

/// Cyclomatic and cognitive complexity.
#[derive(Debug, Default)]
pub struct ComplexityAnalyzer;

impl ComplexityAnalyzer {
    fn cyclomatic(body: &BodyFacts) -> u32 {
        1 + body
            .decisions
            .iter()
            .map(|point| point.kind.paths())
            .sum::<u32>()
    }

    fn cognitive(body: &BodyFacts) -> u32 {
        body.decisions
            .iter()
            .map(|point| point.kind.points(point.nesting))
            .sum()
    }
}

impl Analyzer for ComplexityAnalyzer {
    fn analyze(&self, input: &AnalysisInput<'_>) -> Vec<Measurement> {
        input
            .model
            .symbols()
            .filter_map(|symbol| symbol.body.as_ref().map(|body| (symbol, body)))
            .flat_map(|(symbol, body)| {
                [
                    measure(symbol, MetricId::Cyclomatic, Self::cyclomatic(body)),
                    measure(symbol, MetricId::Cognitive, Self::cognitive(body)),
                ]
            })
            .collect()
    }
}

/// Lines, parameters, nesting and the size of types.
#[derive(Debug, Default)]
pub struct SizeAnalyzer;

impl SizeAnalyzer {
    fn files(input: &AnalysisInput<'_>) -> impl Iterator<Item = Measurement> {
        input.counted_files().flat_map(|file| {
            let lines = file.counted_lines(input.tests);
            [
                (MetricId::Lines, lines.code),
                (MetricId::CommentLines, lines.comment),
                (MetricId::BlankLines, lines.blank),
            ]
            .map(|(metric, value)| Measurement {
                metric,
                subject: Subject::File(file.path.clone()),
                value: f64::from(value),
            })
        })
    }

    fn functions(input: &AnalysisInput<'_>) -> impl Iterator<Item = Measurement> {
        input
            .model
            .symbols()
            .filter_map(|symbol| symbol.body.as_ref().map(|body| (symbol, body)))
            .flat_map(|(symbol, body)| {
                [
                    measure(symbol, MetricId::Lines, symbol.code_lines),
                    measure(symbol, MetricId::Parameters, body.parameters),
                    measure(symbol, MetricId::MaxNesting, body.max_nesting),
                ]
            })
    }

    fn types(input: &AnalysisInput<'_>) -> impl Iterator<Item = Measurement> {
        input
            .model
            .symbols()
            .filter(|symbol| symbol.kind.is_type())
            .flat_map(|symbol| {
                let members = input
                    .model
                    .children(&symbol.id)
                    .filter(|child| matches!(child.kind, SymbolKind::Field | SymbolKind::Method))
                    .fold(0, |count, _| count + 1);
                [
                    measure(symbol, MetricId::Members, members),
                    measure(symbol, MetricId::Lines, symbol.code_lines),
                ]
            })
    }
}

impl Analyzer for SizeAnalyzer {
    fn analyze(&self, input: &AnalysisInput<'_>) -> Vec<Measurement> {
        Self::files(input)
            .chain(Self::functions(input))
            .chain(Self::types(input))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyclops_domain::facts::{DecisionKind, DecisionPoint};

    fn body(points: &[(DecisionKind, u32)]) -> BodyFacts {
        BodyFacts {
            decisions: points
                .iter()
                .map(|&(kind, nesting)| DecisionPoint {
                    kind,
                    line: 1,
                    nesting,
                })
                .collect(),
            ..BodyFacts::default()
        }
    }

    #[test]
    fn cyclomatic_starts_at_one_and_cognitive_at_zero() {
        let empty = body(&[]);
        assert_eq!(ComplexityAnalyzer::cyclomatic(&empty), 1);
        assert_eq!(ComplexityAnalyzer::cognitive(&empty), 0);
    }

    #[test]
    fn nesting_only_costs_cognitive_points() {
        let nested = body(&[(DecisionKind::If, 0), (DecisionKind::If, 2)]);
        assert_eq!(ComplexityAnalyzer::cyclomatic(&nested), 3);
        assert_eq!(ComplexityAnalyzer::cognitive(&nested), 4);
    }

    type Uses = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    );

    fn groups(fields: &[&str], uses: &[Uses]) -> Option<u32> {
        let bodies: Vec<(&str, BodyFacts)> = uses
            .iter()
            .map(|(name, fields, calls)| {
                let body = BodyFacts {
                    fields_used: fields.iter().map(ToString::to_string).collect(),
                    methods_called: calls.iter().map(ToString::to_string).collect(),
                    ..BodyFacts::default()
                };
                (*name, body)
            })
            .collect();
        let methods: Vec<(&str, &BodyFacts)> =
            bodies.iter().map(|(name, body)| (*name, body)).collect();
        CohesionAnalyzer::groups(fields, &methods)
    }

    #[test]
    fn methods_join_through_fields_and_calls_even_indirectly() {
        let fields = ["a", "b", "c"];
        let apart: [Uses; 3] = [("x", &["a"], &[]), ("y", &["b"], &[]), ("z", &["c"], &[])];
        assert_eq!(groups(&fields, &apart), Some(3));
        let chained: [Uses; 3] = [
            ("x", &["a"], &[]),
            ("y", &["a", "b"], &[]),
            ("z", &["b"], &[]),
        ];
        assert_eq!(groups(&fields, &chained), Some(1));
        let called: [Uses; 3] = [("x", &["a"], &["z"]), ("y", &["b"], &[]), ("z", &[], &[])];
        assert_eq!(groups(&fields, &called), Some(2));
    }

    #[test]
    fn a_name_that_is_not_a_field_joins_nothing() {
        let uses: [Uses; 2] = [("x", &["len"], &[]), ("y", &["len"], &[])];
        assert_eq!(groups(&["a"], &uses), Some(2));
    }

    #[test]
    fn no_fields_or_no_methods_is_no_value() {
        let one: [Uses; 1] = [("x", &[], &[])];
        assert_eq!(groups(&[], &one), None);
        assert_eq!(groups(&["a"], &[]), None);
    }

    #[test]
    fn else_is_never_charged_for_nesting() {
        let nested = body(&[(DecisionKind::Else, 3), (DecisionKind::ElseIf, 3)]);
        assert_eq!(ComplexityAnalyzer::cognitive(&nested), 2);
    }
}

/// LCOM4: how many groups of methods have nothing to do with each other.
#[derive(Debug, Default)]
pub struct CohesionAnalyzer;

impl CohesionAnalyzer {
    /// The groups of methods, or nothing for a type with no fields or no methods.
    fn groups(fields: &[&str], methods: &[(&str, &BodyFacts)]) -> Option<u32> {
        if fields.is_empty() || methods.is_empty() {
            return None;
        }
        let mut group: Vec<usize> = (0..methods.len()).collect();
        let find = |group: &mut Vec<usize>, start: usize| {
            let mut root = start;
            while let Some(next) = group.get(root).copied().filter(|next| *next != root) {
                root = next;
            }
            root
        };
        for (a, (name_a, body_a)) in methods.iter().enumerate() {
            for (b, (name_b, body_b)) in methods.iter().enumerate().skip(a + 1) {
                let shares_a_field = body_a.fields_used.iter().any(|field| {
                    fields.contains(&field.as_str()) && body_b.fields_used.contains(field)
                });
                let calls = body_a.methods_called.iter().any(|called| called == name_b)
                    || body_b.methods_called.iter().any(|called| called == name_a);
                if shares_a_field || calls {
                    let (root_a, root_b) = (find(&mut group, a), find(&mut group, b));
                    if let Some(slot) = group.get_mut(root_a) {
                        *slot = root_b;
                    }
                }
            }
        }
        let roots: std::collections::HashSet<usize> = (0..methods.len())
            .map(|method| find(&mut group, method))
            .collect();
        u32::try_from(roots.len()).ok()
    }
}

impl Analyzer for CohesionAnalyzer {
    fn analyze(&self, input: &AnalysisInput<'_>) -> Vec<Measurement> {
        let empty = BodyFacts::default();
        input
            .model
            .symbols()
            .filter(|symbol| matches!(symbol.kind, SymbolKind::Struct | SymbolKind::Class))
            .filter_map(|symbol| {
                let children: Vec<_> = input.model.children(&symbol.id).collect();
                let fields: Vec<&str> = children
                    .iter()
                    .filter(|child| child.kind == SymbolKind::Field)
                    .map(|child| child.name.as_str())
                    .collect();
                let methods: Vec<(&str, &BodyFacts)> = children
                    .iter()
                    .filter(|child| {
                        child.kind == SymbolKind::Method
                            && child.member.instance
                            && !child.member.for_interface
                    })
                    .map(|child| (child.name.as_str(), child.body.as_ref().unwrap_or(&empty)))
                    .collect();
                let groups = Self::groups(&fields, &methods)?;
                Some(measure(symbol, MetricId::Lcom, groups))
            })
            .collect()
    }
}
