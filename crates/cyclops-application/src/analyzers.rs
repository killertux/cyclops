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
            .map(|symbol| {
                let members = input
                    .model
                    .children(&symbol.id)
                    .filter(|child| matches!(child.kind, SymbolKind::Field | SymbolKind::Method))
                    .fold(0, |count, _| count + 1);
                measure(symbol, MetricId::Members, members)
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

    #[test]
    fn else_is_never_charged_for_nesting() {
        let nested = body(&[(DecisionKind::Else, 3), (DecisionKind::ElseIf, 3)]);
        assert_eq!(ComplexityAnalyzer::cognitive(&nested), 2);
    }
}
