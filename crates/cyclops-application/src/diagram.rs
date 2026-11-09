//! Diagrams: the structure of a project with metrics attached. A diagram has no layout,
//! which each interface does its own way. See `specs/explore/diagrams.md`.

use crate::analysis::ProjectAnalysis;
use crate::graph::ModuleGraph;
use cyclops_domain::filter::Filter;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::relation::{Confidence, RelationKind};
use cyclops_domain::symbol::{Symbol, SymbolId, SymbolKind, Visibility};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// The level a diagram shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Only the project itself, which a diagram never draws as a node.
    Project,
    Package,
    Module,
    Type,
    Member,
}

impl Level {
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "package" => Some(Self::Package),
            "module" => Some(Self::Module),
            "type" => Some(Self::Type),
            "member" => Some(Self::Member),
            _ => None,
        }
    }

    #[must_use]
    pub const fn of(kind: SymbolKind) -> Self {
        match kind {
            SymbolKind::Project => Self::Project,
            SymbolKind::Package => Self::Package,
            SymbolKind::Module => Self::Module,
            SymbolKind::Struct
            | SymbolKind::Class
            | SymbolKind::Enum
            | SymbolKind::Interface
            | SymbolKind::Alias => Self::Type,
            SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Field
            | SymbolKind::Constant => Self::Member,
        }
    }
}

/// What to draw.
#[derive(Debug, Clone)]
pub struct DiagramRequest {
    pub level: Level,
    /// Where to start. Nothing means every symbol of the level.
    pub focus: Option<SymbolId>,
    /// How many relations to follow from the focus, in both directions.
    pub depth: u32,
    /// The kinds of relations to show. Empty means all.
    pub relations: Vec<RelationKind>,
    pub overlay: Option<MetricId>,
    pub filter: Option<Filter>,
}

/// A member of a class box.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub name: String,
    pub visibility: Visibility,
    pub overlay: Option<f64>,
    pub alerted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: SymbolId,
    pub kind: SymbolKind,
    pub overlay: Option<f64>,
    pub alerted: bool,
    pub in_cycle: bool,
    /// `interface`, `enum` or `alias`, for a type that is one.
    pub stereotype: Option<&'static str>,
    pub fields: Vec<Member>,
    pub methods: Vec<Member>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: RelationKind,
    /// The weakest of the relations the edge joins.
    pub confidence: Confidence,
    pub in_cycle: bool,
    /// The members the edge came from, when it joins relations between members.
    pub from_members: Vec<String>,
    pub to_members: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiagramView {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl DiagramView {
    #[must_use]
    pub fn node(&self, id: &SymbolId) -> Option<&Node> {
        self.nodes.iter().find(|node| &node.id == id)
    }
}

const fn stereotype(kind: SymbolKind) -> Option<&'static str> {
    match kind {
        SymbolKind::Interface => Some("interface"),
        SymbolKind::Enum => Some("enum"),
        SymbolKind::Alias => Some("alias"),
        _ => None,
    }
}

/// An edge as it builds up.
struct Joined {
    confidence: Confidence,
    from_members: BTreeSet<String>,
    to_members: BTreeSet<String>,
}

impl ProjectAnalysis {
    fn alerted(&self, id: &SymbolId) -> bool {
        self.alerts()
            .iter()
            .any(|alert| alert.subject == Subject::Symbol(id.clone()))
    }

    /// The symbol a relation end stands for at a level: itself, or the type a member
    /// belongs to.
    fn lifted(&self, id: &SymbolId, level: Level) -> Option<(SymbolId, bool)> {
        let view = self.view();
        let symbol = view.symbol(id)?;
        if Level::of(symbol.kind) == level {
            return Some((id.clone(), false));
        }
        if level != Level::Type || Level::of(symbol.kind) != Level::Member {
            return None;
        }
        let mut current = symbol.parent.as_ref();
        while let Some(parent) = current {
            let owner = view.symbol(parent)?;
            if owner.kind.is_type() {
                return Some((parent.clone(), true));
            }
            current = owner.parent.as_ref();
        }
        None
    }

    fn edges_at(
        &self,
        level: Level,
        kinds: &[RelationKind],
    ) -> BTreeMap<(SymbolId, SymbolId, RelationKind), Joined> {
        let mut edges: BTreeMap<(SymbolId, SymbolId, RelationKind), Joined> = BTreeMap::new();
        let view = self.view();
        for relation in view.relations() {
            if !kinds.is_empty() && !kinds.contains(&relation.kind) {
                continue;
            }
            let (Some((from, from_joined)), Some((to, to_joined))) = (
                self.lifted(&relation.from, level),
                self.lifted(&relation.to, level),
            ) else {
                continue;
            };
            if from == to {
                continue;
            }
            let edge = edges
                .entry((from, to, relation.kind))
                .or_insert_with(|| Joined {
                    confidence: relation.confidence,
                    from_members: BTreeSet::new(),
                    to_members: BTreeSet::new(),
                });
            edge.confidence = edge.confidence.max(relation.confidence);
            let name = |id: &SymbolId| view.symbol(id).map(|symbol| symbol.name.clone());
            if from_joined {
                edge.from_members.extend(name(&relation.from));
            }
            if to_joined {
                edge.to_members.extend(name(&relation.to));
            }
        }
        edges
    }

    /// The nodes to start from: the focus lifted to the level, or what is inside it.
    fn starts(&self, request: &DiagramRequest) -> BTreeSet<SymbolId> {
        let view = self.view();
        // A folder that only holds modules is not drawn as a module of its own.
        let at_level: Vec<&Symbol> = view
            .symbols()
            .filter(|symbol| Level::of(symbol.kind) == request.level)
            .filter(|symbol| symbol.kind != SymbolKind::Module || symbol.location.is_some())
            .collect();
        let Some(focus) = &request.focus else {
            return at_level.iter().map(|symbol| symbol.id.clone()).collect();
        };
        let Some(focused) = view.symbol(focus) else {
            return BTreeSet::new();
        };
        if Level::of(focused.kind) < request.level {
            return at_level
                .iter()
                .filter(|symbol| view.is_within(&symbol.id, focus))
                .map(|symbol| symbol.id.clone())
                .collect();
        }
        let mut current = Some(focused);
        while let Some(symbol) = current {
            if Level::of(symbol.kind) == request.level {
                return BTreeSet::from([symbol.id.clone()]);
            }
            current = symbol
                .parent
                .as_ref()
                .and_then(|parent| view.symbol(parent));
        }
        BTreeSet::new()
    }

    /// Draws a diagram.
    #[must_use]
    pub fn diagram(&self, request: &DiagramRequest) -> DiagramView {
        let view = self.view();
        let all_edges = self.edges_at(request.level, &request.relations);
        let mut chosen = self.starts(request);
        if request.focus.is_some() {
            let mut frontier = chosen.clone();
            for _ in 0..request.depth {
                let mut next = BTreeSet::new();
                for (from, to, _) in all_edges.keys() {
                    if frontier.contains(from) && !chosen.contains(to) {
                        next.insert(to.clone());
                    }
                    if frontier.contains(to) && !chosen.contains(from) {
                        next.insert(from.clone());
                    }
                }
                chosen.extend(next.iter().cloned());
                frontier = next;
            }
        }
        if let Some(filter) = &request.filter {
            let matches: Vec<SymbolId> = self
                .search(filter, None, None)
                .iter()
                .map(|symbol| symbol.id.clone())
                .collect();
            chosen.retain(|node| matches.iter().any(|found| view.is_within(found, node)));
        }
        let cycles: Vec<Vec<SymbolId>> = ModuleGraph::of(&view).cycles();
        let cycle_of = |id: &SymbolId| cycles.iter().position(|cycle| cycle.contains(id));
        let in_cycle: HashSet<&SymbolId> = cycles.iter().flatten().collect();
        let overlay = |id: &SymbolId| {
            request
                .overlay
                .and_then(|metric| self.value(&Subject::Symbol(id.clone()), metric))
        };
        let nodes = chosen
            .iter()
            .filter_map(|id| view.symbol(id))
            .map(|symbol| {
                let members = |kind: SymbolKind| {
                    let mut found: Vec<Member> = view
                        .children(&symbol.id)
                        .filter(|child| child.kind == kind)
                        .map(|child| Member {
                            name: child.name.clone(),
                            visibility: child.visibility,
                            overlay: overlay(&child.id),
                            alerted: self.alerted(&child.id),
                        })
                        .collect();
                    found.sort_by(|a, b| a.name.cmp(&b.name));
                    found
                };
                let is_type = symbol.kind.is_type();
                Node {
                    id: symbol.id.clone(),
                    kind: symbol.kind,
                    overlay: overlay(&symbol.id),
                    alerted: self.alerted(&symbol.id),
                    in_cycle: in_cycle.contains(&symbol.id),
                    stereotype: stereotype(symbol.kind),
                    fields: if is_type {
                        members(SymbolKind::Field)
                    } else {
                        Vec::new()
                    },
                    methods: if is_type {
                        members(SymbolKind::Method)
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect();
        let edges = all_edges
            .into_iter()
            .filter(|((from, to, _), _)| chosen.contains(from) && chosen.contains(to))
            .map(|((from, to, kind), joined)| Edge {
                in_cycle: cycle_of(&from).is_some() && cycle_of(&from) == cycle_of(&to),
                from,
                to,
                kind,
                confidence: joined.confidence,
                from_members: joined.from_members.into_iter().collect(),
                to_members: joined.to_members.into_iter().collect(),
            })
            .collect();
        DiagramView { nodes, edges }
    }
}
