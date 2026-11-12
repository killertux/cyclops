//! The modules of a project as a graph of imports: coupling, and dependency cycles.

use crate::analysis::AnalysisInput;
use crate::ports::Analyzer;
use cyclops_domain::metric::{Measurement, MetricId, Subject};
use cyclops_domain::model::ModelView;
use cyclops_domain::relation::RelationKind;
use cyclops_domain::symbol::{SymbolId, SymbolKind};
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{BTreeMap, BTreeSet};

/// Which modules each module imports, among the visible ones.
#[derive(Debug, Clone, Default)]
pub struct ModuleGraph {
    imports: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
}

impl ModuleGraph {
    #[must_use]
    pub fn of(view: &ModelView<'_>) -> Self {
        let mut imports: BTreeMap<SymbolId, BTreeSet<SymbolId>> = view
            .symbols()
            .filter(|symbol| symbol.kind == SymbolKind::Module)
            .map(|module| (module.id.clone(), BTreeSet::new()))
            .collect();
        for relation in view
            .relations()
            .filter(|relation| relation.kind == RelationKind::Imports)
        {
            if relation.from != relation.to {
                imports
                    .entry(relation.from.clone())
                    .or_default()
                    .insert(relation.to.clone());
                imports.entry(relation.to.clone()).or_default();
            }
        }
        Self { imports }
    }

    /// The modules a module imports, in order.
    #[must_use]
    pub fn depends_on(&self, module: &SymbolId) -> Vec<&SymbolId> {
        self.imports.get(module).into_iter().flatten().collect()
    }

    /// The modules that import a module, in order.
    #[must_use]
    pub fn depended_on_by(&self, module: &SymbolId) -> Vec<&SymbolId> {
        self.imports
            .iter()
            .filter(|(_, targets)| targets.contains(module))
            .map(|(from, _)| from)
            .collect()
    }

    fn efferent(&self, module: &SymbolId) -> usize {
        self.imports.get(module).map_or(0, BTreeSet::len)
    }

    fn afferent(&self, module: &SymbolId) -> usize {
        self.imports
            .values()
            .filter(|targets| targets.contains(module))
            .count()
    }

    /// Each group of modules that all depend on each other, in order, with its members
    /// in order.
    #[must_use]
    pub fn cycles(&self) -> Vec<Vec<SymbolId>> {
        let mut graph = DiGraph::<SymbolId, ()>::new();
        let nodes: BTreeMap<&SymbolId, NodeIndex> = self
            .imports
            .keys()
            .map(|module| (module, graph.add_node(module.clone())))
            .collect();
        for (from, targets) in &self.imports {
            for to in targets {
                if let (Some(from), Some(to)) = (nodes.get(from), nodes.get(to)) {
                    graph.add_edge(*from, *to, ());
                }
            }
        }
        let mut cycles: Vec<Vec<SymbolId>> = tarjan_scc(&graph)
            .into_iter()
            .filter(|component| component.len() > 1)
            .map(|component| {
                let mut members: Vec<SymbolId> = component
                    .into_iter()
                    .filter_map(|node| graph.node_weight(node).cloned())
                    .collect();
                members.sort();
                members
            })
            .collect();
        cycles.sort();
        cycles
    }
}

fn ratio(part: usize, whole: usize) -> Option<f64> {
    let part = u32::try_from(part).ok()?;
    let whole = u32::try_from(whole).ok()?;
    (whole > 0).then(|| f64::from(part) / f64::from(whole))
}

fn measure(module: &SymbolId, metric: MetricId, value: f64) -> Measurement {
    Measurement {
        metric,
        subject: Subject::Symbol(module.clone()),
        value,
    }
}

/// Afferent and efferent coupling, instability, abstractness, distance, and cycles.
#[derive(Debug, Default)]
pub struct CouplingAnalyzer;

impl Analyzer for CouplingAnalyzer {
    fn analyze(&self, input: &AnalysisInput<'_>) -> Vec<Measurement> {
        let graph = ModuleGraph::of(&input.model);
        let mut measurements = Vec::new();
        for module in input
            .model
            .symbols()
            .filter(|symbol| symbol.kind == SymbolKind::Module)
        {
            let id = &module.id;
            let (afferent, efferent) = (graph.afferent(id), graph.efferent(id));
            let count = |value: usize| f64::from(u32::try_from(value).unwrap_or(u32::MAX));
            measurements.push(measure(id, MetricId::Afferent, count(afferent)));
            measurements.push(measure(id, MetricId::Efferent, count(efferent)));
            let instability = ratio(efferent, afferent + efferent);
            if let Some(instability) = instability {
                measurements.push(measure(id, MetricId::Instability, instability));
            }
            let types: Vec<SymbolKind> = input
                .model
                .children(id)
                .map(|child| child.kind)
                .filter(|kind| kind.is_type())
                .collect();
            let interfaces = types
                .iter()
                .filter(|kind| **kind == SymbolKind::Interface)
                .count();
            let abstractness = ratio(interfaces, types.len());
            if let Some(abstractness) = abstractness {
                measurements.push(measure(id, MetricId::Abstractness, abstractness));
            }
            if let (Some(abstractness), Some(instability)) = (abstractness, instability) {
                let distance = (abstractness + instability - 1.0).abs();
                measurements.push(measure(id, MetricId::Distance, distance));
            }
        }
        for cycle in graph.cycles() {
            let size = f64::from(u32::try_from(cycle.len()).unwrap_or(u32::MAX));
            for module in &cycle {
                measurements.push(measure(module, MetricId::CycleSize, size));
            }
        }
        measurements
    }
}
