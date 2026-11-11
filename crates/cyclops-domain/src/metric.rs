//! The metric catalog. Each metric describes itself, so filters, alerts and interfaces
//! all treat metrics the same way. See `specs/metrics/catalog.md`.

use crate::path::RelativePath;
use crate::symbol::SymbolId;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MetricId {
    Cyclomatic,
    Cognitive,
    MaxNesting,
    Lines,
    CommentLines,
    BlankLines,
    Parameters,
    Members,
    Duplication,
    Lcom,
    Afferent,
    Efferent,
    Instability,
    Abstractness,
    Distance,
    CycleSize,
    Churn,
    Hotspot,
}

impl MetricId {
    pub const ALL: [Self; 18] = [
        Self::Cyclomatic,
        Self::Cognitive,
        Self::MaxNesting,
        Self::Lines,
        Self::CommentLines,
        Self::BlankLines,
        Self::Parameters,
        Self::Members,
        Self::Duplication,
        Self::Lcom,
        Self::Afferent,
        Self::Efferent,
        Self::Instability,
        Self::Abstractness,
        Self::Distance,
        Self::CycleSize,
        Self::Churn,
        Self::Hotspot,
    ];

    /// What it measures, as the catalog says.
    #[must_use]
    pub const fn measures(self) -> &'static str {
        match self {
            Self::Cyclomatic => "Paths through code",
            Self::Cognitive => "Difficulty to follow",
            Self::MaxNesting => "Deepest control flow",
            Self::Lines => "Lines of code",
            Self::CommentLines => "Lines of comments",
            Self::BlankLines => "Blank lines",
            Self::Parameters => "Function parameters",
            Self::Members => "Fields and methods",
            Self::Duplication => "Share of copied lines",
            Self::Lcom => "Unrelated method groups",
            Self::Afferent => "Modules depending on it",
            Self::Efferent => "Modules it depends on",
            Self::Instability => "Share of outgoing dependencies",
            Self::Abstractness => "Share of interfaces",
            Self::Distance => "Imbalance of the two",
            Self::CycleSize => "Modules in its cycle",
            Self::Churn => "Commits changing it",
            Self::Hotspot => "Churn and complexity",
        }
    }

    /// Finds a metric by the name filters and alerts use.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.name() == name)
    }

    /// The name filters and alerts use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cyclomatic => "cyclomatic",
            Self::Cognitive => "cognitive",
            Self::MaxNesting => "max_nesting",
            Self::Lines => "lines",
            Self::CommentLines => "comment_lines",
            Self::BlankLines => "blank_lines",
            Self::Parameters => "parameters",
            Self::Members => "members",
            Self::Duplication => "duplication",
            Self::Lcom => "lcom",
            Self::Afferent => "afferent",
            Self::Efferent => "efferent",
            Self::Instability => "instability",
            Self::Abstractness => "abstractness",
            Self::Distance => "distance",
            Self::CycleSize => "cycle_size",
            Self::Churn => "churn",
            Self::Hotspot => "hotspot",
        }
    }

    /// What the catalog says about this metric.
    #[must_use]
    pub const fn descriptor(self) -> MetricDescriptor {
        use Direction::{LowerIsBetter, Neutral};
        const fn neutral() -> (Direction, Option<HeatBands>) {
            (Neutral, None)
        }
        const fn lower(mild: f64, high: f64, severe: f64) -> (Direction, Option<HeatBands>) {
            let only_on = None;
            let bands = HeatBands {
                mild,
                high,
                severe,
                only_on,
            };
            (LowerIsBetter, Some(bands))
        }
        let (direction, heat) = match self {
            Self::Cyclomatic => lower(5.0, 10.0, 20.0),
            Self::Cognitive => lower(8.0, 15.0, 25.0),
            Self::MaxNesting => lower(3.0, 4.0, 5.0),
            Self::Lines => {
                let bands = HeatBands {
                    mild: 30.0,
                    high: 60.0,
                    severe: 100.0,
                    only_on: Some(Scope::Function),
                };
                (LowerIsBetter, Some(bands))
            }
            Self::Parameters => lower(4.0, 6.0, 8.0),
            Self::Members => lower(15.0, 25.0, 40.0),
            Self::Duplication => lower(5.0, 15.0, 30.0),
            Self::Lcom => lower(2.0, 3.0, 5.0),
            Self::Distance => lower(0.30, 0.50, 0.70),
            Self::CycleSize => lower(2.0, 4.0, 8.0),
            Self::Hotspot => lower(0.20, 0.40, 0.70),
            Self::CommentLines
            | Self::BlankLines
            | Self::Afferent
            | Self::Efferent
            | Self::Instability
            | Self::Abstractness
            | Self::Churn => neutral(),
        };
        let ratio = matches!(
            self,
            Self::Instability
                | Self::Abstractness
                | Self::Distance
                | Self::Hotspot
                | Self::Duplication
        );
        let rollup = match self {
            Self::Cyclomatic | Self::Cognitive | Self::MaxNesting | Self::Churn | Self::Hotspot => {
                Rollup::Max
            }
            Self::Lines | Self::CommentLines | Self::BlankLines => Rollup::Sum,
            _ => Rollup::None,
        };
        MetricDescriptor {
            id: self,
            direction,
            heat,
            ratio,
            rollup,
        }
    }
}

impl fmt::Display for MetricId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What a metric can be measured on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Function,
    Type,
    File,
    Module,
    Project,
}

impl Scope {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Type => "type",
            Self::File => "file",
            Self::Module => "module",
            Self::Project => "project",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "function" => Some(Self::Function),
            "type" => Some(Self::Type),
            "file" => Some(Self::File),
            "module" => Some(Self::Module),
            "project" => Some(Self::Project),
            _ => None,
        }
    }
}

/// Whether a value is good or bad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    LowerIsBetter,
    /// It only describes something, and neither end is a problem.
    Neutral,
}

impl Direction {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LowerIsBetter => "lower is better",
            Self::Neutral => "neutral",
        }
    }
}

/// How worrying a value is, from fixed reference values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heat {
    Quiet,
    Mild,
    High,
    Severe,
}

impl Heat {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Quiet => "quiet",
            Self::Mild => "mild",
            Self::High => "high",
            Self::Severe => "severe",
        }
    }
}

/// Where each level of heat starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeatBands {
    pub mild: f64,
    pub high: f64,
    pub severe: f64,
    /// The only scope these apply to, if they do not apply to all of them.
    pub only_on: Option<Scope>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricDescriptor {
    pub id: MetricId,
    pub direction: Direction,
    /// Reference values. Neutral metrics have none.
    pub heat: Option<HeatBands>,
    /// Whether values are ratios or shares, written with two decimal places.
    pub ratio: bool,
    /// How it appears on larger symbols.
    pub rollup: Rollup,
}

/// How a metric of small symbols appears on the larger ones that contain them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rollup {
    None,
    /// The highest of its parts.
    Max,
    /// The sum of its parts.
    Sum,
}

impl MetricDescriptor {
    /// The scope a metric is measured on, before it rolls up.
    #[must_use]
    pub const fn measured_on(&self) -> Scope {
        match self.id {
            MetricId::Cyclomatic
            | MetricId::Cognitive
            | MetricId::MaxNesting
            | MetricId::Lines
            | MetricId::Parameters => Scope::Function,
            MetricId::Members | MetricId::Lcom => Scope::Type,
            MetricId::CommentLines
            | MetricId::BlankLines
            | MetricId::Duplication
            | MetricId::Churn
            | MetricId::Hotspot => Scope::File,
            MetricId::Afferent
            | MetricId::Efferent
            | MetricId::Instability
            | MetricId::Abstractness
            | MetricId::Distance
            | MetricId::CycleSize => Scope::Module,
        }
    }

    /// The finest scope a metric has values on. Below it nothing has a value, so the
    /// tree and the map stop there. See `specs/tui/explore.md`.
    #[must_use]
    pub const fn finest_scope(&self) -> Scope {
        match self.id {
            // Copies are also found within functions.
            MetricId::Duplication => Scope::Function,
            _ => self.measured_on(),
        }
    }

    /// Whether a metric has values at a scope, measured there or rolled up to it.
    #[must_use]
    pub fn applies_to(&self, scope: Scope) -> bool {
        let rolled = match self.id {
            MetricId::Cyclomatic | MetricId::Lines => {
                &[Scope::Type, Scope::Module, Scope::File][..]
            }
            MetricId::Cognitive | MetricId::MaxNesting => &[Scope::Type, Scope::Module][..],
            MetricId::CommentLines
            | MetricId::BlankLines
            | MetricId::Duplication
            | MetricId::Churn
            | MetricId::Hotspot => &[Scope::Module][..],
            _ => &[][..],
        };
        self.measured_on() == scope || rolled.contains(&scope)
    }

    /// The heat of a value of this metric, if it has any.
    #[must_use]
    pub fn heat_of(&self, scope: Scope, value: f64) -> Option<Heat> {
        let bands = self.heat?;
        if bands.only_on.is_some_and(|only| only != scope) {
            return None;
        }
        Some(if value >= bands.severe {
            Heat::Severe
        } else if value >= bands.high {
            Heat::High
        } else if value >= bands.mild {
            Heat::Mild
        } else {
            Heat::Quiet
        })
    }

    /// A value the way every interface writes it.
    #[must_use]
    pub fn format(&self, value: f64) -> String {
        if self.ratio {
            format!("{value:.2}")
        } else {
            format!("{value:.0}")
        }
    }
}

/// What a measurement is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Subject {
    Symbol(SymbolId),
    File(RelativePath),
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(id) => id.fmt(f),
            Self::File(path) => path.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub metric: MetricId,
    pub subject: Subject,
    pub value: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_metric_can_be_found_by_its_name() {
        for id in MetricId::ALL {
            assert_eq!(MetricId::parse(id.name()), Some(id));
        }
        assert_eq!(MetricId::parse("nonsense"), None);
    }

    #[test]
    fn only_neutral_metrics_have_no_heat() {
        for id in MetricId::ALL {
            let descriptor = id.descriptor();
            assert_eq!(
                descriptor.heat.is_none(),
                descriptor.direction == Direction::Neutral,
                "{id}"
            );
        }
    }

    #[test]
    fn ratios_have_two_decimal_places() {
        assert_eq!(MetricId::Instability.descriptor().format(1.0), "1.00");
        assert_eq!(MetricId::Cyclomatic.descriptor().format(3.0), "3");
    }
}
