//! Diagrams: the structure of a project with metrics attached. A diagram has no layout,
//! which each interface does its own way. See `specs/explore/diagrams.md`.

use crate::analysis::ProjectAnalysis;
use cyclops_domain::filter::Filter;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::relation::{Confidence, Relation, RelationKind};
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
/// The edges of a diagram, each with what joined into it.
type Edges = BTreeMap<(SymbolId, SymbolId, RelationKind), Joined>;

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

    /// Every edge at a level, between any nodes.
    fn edges_at(&self, level: Level, kinds: &[RelationKind]) -> Edges {
        let mut edges = Edges::new();
        for relation in self.view().relations() {
            self.join(&mut edges, relation, level, kinds, None);
        }
        edges
    }

    /// The edges at a level between the nodes, found from the relations of what each
    /// node stands for rather than from every relation of the project.
    fn edges_among(
        &self,
        level: Level,
        kinds: &[RelationKind],
        nodes: &BTreeSet<SymbolId>,
    ) -> Edges {
        let view = self.view();
        let mut edges = Edges::new();
        for node in nodes {
            for id in self.lifted_to(node, level) {
                for relation in view.relations_from(&id) {
                    self.join(&mut edges, relation, level, kinds, Some(nodes));
                }
            }
        }
        edges
    }

    /// Adds a relation to the edge it lifts to at a level, if it is of a kind drawn and
    /// joins two nodes, both among `among` when it is given.
    fn join(
        &self,
        edges: &mut Edges,
        relation: &Relation,
        level: Level,
        kinds: &[RelationKind],
        among: Option<&BTreeSet<SymbolId>>,
    ) {
        if !kinds.is_empty() && !kinds.contains(&relation.kind) {
            return;
        }
        let (Some((from, from_joined)), Some((to, to_joined))) = (
            self.lifted(&relation.from, level),
            self.lifted(&relation.to, level),
        ) else {
            return;
        };
        if from == to || among.is_some_and(|among| !among.contains(&from) || !among.contains(&to)) {
            return;
        }
        let view = self.view();
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

    /// Every symbol that lifts to a node at a level: the node, and at the level of types
    /// the members whose nearest type it is.
    fn lifted_to(&self, node: &SymbolId, level: Level) -> Vec<SymbolId> {
        let view = self.view();
        let mut found = vec![node.clone()];
        if level != Level::Type {
            return found;
        }
        let mut pending: Vec<&Symbol> = view.children(node).collect();
        while let Some(symbol) = pending.pop() {
            if symbol.kind.is_type() {
                continue;
            }
            if Level::of(symbol.kind) == Level::Member {
                found.push(symbol.id.clone());
            }
            pending.extend(view.children(&symbol.id));
        }
        found
    }

    /// The nodes at a level one relation away from any of `nodes`, either way.
    fn neighbors(
        &self,
        level: Level,
        kinds: &[RelationKind],
        nodes: &BTreeSet<SymbolId>,
    ) -> BTreeSet<SymbolId> {
        let view = self.view();
        let mut found = BTreeSet::new();
        for node in nodes {
            for id in self.lifted_to(node, level) {
                let ends = view
                    .relations_from(&id)
                    .map(|relation| (&relation.to, relation.kind))
                    .chain(
                        view.relations_to(&id)
                            .map(|relation| (&relation.from, relation.kind)),
                    );
                for (end, kind) in ends {
                    if !kinds.is_empty() && !kinds.contains(&kind) {
                        continue;
                    }
                    if let Some((other, _)) = self.lifted(end, level)
                        && &other != node
                    {
                        found.insert(other);
                    }
                }
            }
        }
        found
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
        let mut chosen = self.starts(request);
        if request.focus.is_some() {
            let mut frontier = chosen.clone();
            for _ in 0..request.depth {
                let next: BTreeSet<SymbolId> = self
                    .neighbors(request.level, &request.relations, &frontier)
                    .into_iter()
                    .filter(|node| !chosen.contains(node))
                    .collect();
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
        let cycles = self.module_cycles();
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
        // Without a focus every node is chosen, and every relation counts.
        let all_edges = if request.focus.is_some() {
            self.edges_among(request.level, &request.relations, &chosen)
        } else {
            self.edges_at(request.level, &request.relations)
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Extras;
    use cyclops_domain::model::{ProjectModel, TestCode};
    use cyclops_domain::symbol::CodeRole;

    fn symbol(id: &str, parent: Option<&str>, kind: SymbolKind, role: CodeRole) -> Symbol {
        let name = id.rsplit("::").next().unwrap_or(id);
        Symbol::container(
            SymbolId::new(id),
            parent.map(SymbolId::new),
            name,
            kind,
            role,
        )
    }

    fn relation(from: &str, to: &str, kind: RelationKind, confidence: Confidence) -> Relation {
        Relation {
            from: SymbolId::new(from),
            to: SymbolId::new(to),
            kind,
            confidence,
        }
    }

    /// Two modules with classes, members, a nested function and test code, related
    /// every way.
    fn project(tests: TestCode) -> ProjectAnalysis {
        use CodeRole::{Production, Test};
        use Confidence::{Ambiguous, Exact, Inferred};
        use RelationKind::{Calls, Composes, Imports, Uses};
        use SymbolKind::{Class, Function, Method, Module, Package};
        let symbols = vec![
            symbol("p", None, Package, Production),
            symbol("p::m1", Some("p"), Module, Production),
            symbol("p::m1::A", Some("p::m1"), Class, Production),
            symbol("p::m1::A::a1", Some("p::m1::A"), Method, Production),
            symbol(
                "p::m1::A::a1::inner",
                Some("p::m1::A::a1"),
                Function,
                Production,
            ),
            symbol("p::m1::A::a2", Some("p::m1::A"), Method, Production),
            symbol("p::m1::f", Some("p::m1"), Function, Production),
            symbol("p::m2", Some("p"), Module, Production),
            symbol("p::m2::B", Some("p::m2"), Class, Production),
            symbol("p::m2::B::b1", Some("p::m2::B"), Method, Production),
            symbol("p::m2::C", Some("p::m2"), Class, Test),
            symbol("p::m2::C::c1", Some("p::m2::C"), Method, Test),
            symbol("p::m3", Some("p"), Module, Production),
            symbol("p::m3::D", Some("p::m3"), Class, Production),
        ];
        let symbols = symbols
            .into_iter()
            .map(|mut symbol| {
                if symbol.kind == Module {
                    symbol.location = Some(cyclops_domain::symbol::Location {
                        file: cyclops_domain::path::RelativePath::new(symbol.id.as_str()),
                        span: cyclops_domain::symbol::Span {
                            start_line: 1,
                            end_line: 1,
                        },
                    });
                }
                symbol
            })
            .collect();
        let relations = vec![
            relation("p::m1::A::a1", "p::m2::B::b1", Calls, Inferred),
            relation("p::m1::A::a1::inner", "p::m2::B::b1", Calls, Exact),
            relation("p::m1::A::a2", "p::m2::B", Uses, Exact),
            relation("p::m1::A::a1", "p::m1::A::a2", Calls, Exact),
            relation("p::m2::B", "p::m1::A", Composes, Ambiguous),
            relation("p::m2::C::c1", "p::m1::A::a1", Calls, Exact),
            relation("p::m3::D", "p::m2::B", Uses, Exact),
            relation("p::m1::f", "p::m1::f", Calls, Exact),
            relation("p::m1", "p::m2", Imports, Exact),
            relation("p::m2", "p::m3", Imports, Exact),
            relation("p::m3", "p::m1", Imports, Inferred),
        ];
        ProjectAnalysis::new(
            ProjectModel::new(symbols, relations),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            tests,
            0,
            Extras::default(),
        )
    }

    /// The diagram as it was drawn before it looked only around the focus: every edge of
    /// the level, followed from the focus.
    fn from_every_edge(analysis: &ProjectAnalysis, request: &DiagramRequest) -> DiagramView {
        let all_edges = analysis.edges_at(request.level, &request.relations);
        let mut chosen = analysis.starts(request);
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
        let edges = all_edges
            .into_iter()
            .filter(|((from, to, _), _)| chosen.contains(from) && chosen.contains(to))
            .map(|((from, to, kind), joined)| Edge {
                in_cycle: false,
                from,
                to,
                kind,
                confidence: joined.confidence,
                from_members: joined.from_members.into_iter().collect(),
                to_members: joined.to_members.into_iter().collect(),
            })
            .collect();
        let nodes = chosen
            .iter()
            .filter_map(|id| analysis.view().symbol(id))
            .map(|symbol| Node {
                id: symbol.id.clone(),
                kind: symbol.kind,
                overlay: None,
                alerted: false,
                in_cycle: false,
                stereotype: None,
                fields: Vec::new(),
                methods: Vec::new(),
            })
            .collect();
        DiagramView { nodes, edges }
    }

    fn outline(view: &DiagramView) -> (Vec<SymbolId>, Vec<Edge>) {
        let edges = view
            .edges
            .iter()
            .map(|edge| Edge {
                in_cycle: false,
                ..edge.clone()
            })
            .collect();
        (
            view.nodes.iter().map(|node| node.id.clone()).collect(),
            edges,
        )
    }

    #[test]
    fn a_focus_draws_what_following_every_edge_would() {
        let kinds = [
            Vec::new(),
            vec![RelationKind::Calls],
            vec![RelationKind::Uses],
        ];
        let levels = [Level::Package, Level::Module, Level::Type, Level::Member];
        for tests in [TestCode::Excluded, TestCode::Included] {
            let analysis = project(tests);
            let ids: Vec<SymbolId> = analysis
                .model()
                .symbols()
                .map(|symbol| symbol.id.clone())
                .collect();
            for focus in &ids {
                for level in levels {
                    for depth in 0..3 {
                        for relations in &kinds {
                            let request = DiagramRequest {
                                level,
                                focus: Some(focus.clone()),
                                depth,
                                relations: relations.clone(),
                                overlay: None,
                                filter: None,
                            };
                            assert_eq!(
                                outline(&analysis.diagram(&request)),
                                outline(&from_every_edge(&analysis, &request)),
                                "{focus:?} at {level:?}, depth {depth}, {relations:?}, {tests:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
