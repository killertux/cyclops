//! The project model: every symbol, after linking.

use crate::relation::{Relation, RelationKind};
use crate::symbol::{CodeRole, Symbol, SymbolId, SymbolKind};
use std::collections::{BTreeMap, HashMap};

/// Whether test code counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TestCode {
    #[default]
    Excluded,
    Included,
}

impl TestCode {
    #[must_use]
    pub const fn shows(self, role: CodeRole) -> bool {
        matches!(
            (self, role),
            (Self::Included, _) | (Self::Excluded, CodeRole::Production)
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectModel {
    /// Every symbol, in order of id.
    symbols: Vec<Symbol>,
    /// Where each symbol is in `symbols`.
    index: HashMap<SymbolId, usize>,
    /// What is directly inside each symbol, as places in `symbols`, in order of id.
    children: HashMap<SymbolId, Vec<usize>>,
    /// The same, for the symbol at each place, to walk down without looking ids up.
    children_at: Vec<Vec<usize>>,
    relations: Vec<Relation>,
    /// Whether each relation has test code at either end, so that a view without test
    /// code can leave it out without looking its ends up.
    touches_tests: Vec<bool>,
    /// Where each relation starts and ends, as places in `symbols`.
    ends: Vec<(usize, usize)>,
    /// The relations that start at each symbol, and that end at it, as places in
    /// `relations`, in order.
    outgoing: HashMap<SymbolId, Vec<usize>>,
    incoming: HashMap<SymbolId, Vec<usize>>,
}

impl ProjectModel {
    /// Builds the model. If two languages describe the same symbol, such as the package
    /// they share, the first one stays. A relation stated twice keeps its surest form,
    /// and one to a symbol that does not exist is dropped.
    #[must_use]
    pub fn new(symbols: Vec<Symbol>, relations: Vec<Relation>) -> Self {
        let mut by_id = BTreeMap::new();
        for symbol in symbols {
            by_id.entry(symbol.id.clone()).or_insert(symbol);
        }
        let symbols: Vec<Symbol> = by_id.into_values().collect();
        let index: HashMap<SymbolId, usize> = symbols
            .iter()
            .enumerate()
            .map(|(place, symbol)| (symbol.id.clone(), place))
            .collect();
        let mut children: HashMap<SymbolId, Vec<usize>> = HashMap::new();
        for (place, symbol) in symbols.iter().enumerate() {
            if let Some(parent) = &symbol.parent {
                children.entry(parent.clone()).or_default().push(place);
            }
        }
        let children_at = symbols
            .iter()
            .map(|symbol| children.get(&symbol.id).cloned().unwrap_or_default())
            .collect();
        let mut best: BTreeMap<(SymbolId, SymbolId, RelationKind), Relation> = BTreeMap::new();
        for relation in relations {
            if relation.from == relation.to && relation.kind != RelationKind::Calls {
                continue;
            }
            if !index.contains_key(&relation.from) || !index.contains_key(&relation.to) {
                continue;
            }
            let key = (relation.from.clone(), relation.to.clone(), relation.kind);
            match best.get(&key) {
                Some(known) if known.confidence <= relation.confidence => {}
                _ => {
                    best.insert(key, relation);
                }
            }
        }
        let relations: Vec<Relation> = best.into_values().collect();
        let is_test = |id: &SymbolId| {
            index
                .get(id)
                .and_then(|place| symbols.get(*place))
                .is_some_and(|symbol| !TestCode::Excluded.shows(symbol.role))
        };
        let touches_tests = relations
            .iter()
            .map(|relation| is_test(&relation.from) || is_test(&relation.to))
            .collect();
        // Every relation kept has both ends among the symbols.
        let ends = relations
            .iter()
            .map(|relation| {
                let place = |id: &SymbolId| index.get(id).copied().unwrap_or_default();
                (place(&relation.from), place(&relation.to))
            })
            .collect();
        let mut outgoing: HashMap<SymbolId, Vec<usize>> = HashMap::new();
        let mut incoming: HashMap<SymbolId, Vec<usize>> = HashMap::new();
        for (place, relation) in relations.iter().enumerate() {
            outgoing
                .entry(relation.from.clone())
                .or_default()
                .push(place);
            incoming.entry(relation.to.clone()).or_default().push(place);
        }
        Self {
            symbols,
            index,
            children,
            children_at,
            relations,
            touches_tests,
            ends,
            outgoing,
            incoming,
        }
    }

    /// Every relation, in order of where it starts.
    pub fn relations(&self) -> impl Iterator<Item = &Relation> {
        self.relations.iter()
    }

    #[must_use]
    pub fn symbol(&self, id: &SymbolId) -> Option<&Symbol> {
        self.index
            .get(id)
            .and_then(|place| self.symbols.get(*place))
    }

    /// Every symbol, in order of id.
    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    /// Where a symbol is among every symbol, in order of id. It stays the same for as
    /// long as the model does, so it can stand for the symbol.
    #[must_use]
    pub fn place(&self, id: &SymbolId) -> Option<usize> {
        self.index.get(id).copied()
    }

    /// The symbol at a place among every symbol.
    #[must_use]
    pub fn at(&self, place: usize) -> Option<&Symbol> {
        self.symbols.get(place)
    }

    /// What is directly inside a symbol, in order of id.
    pub fn children(&self, id: &SymbolId) -> impl Iterator<Item = &Symbol> {
        self.children
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|place| self.symbols.get(*place))
    }

    /// The relations at these places, with whether each has test code at either end.
    fn relations_at<'a>(
        &'a self,
        places: Option<&'a Vec<usize>>,
    ) -> impl Iterator<Item = (&'a Relation, bool)> {
        places.into_iter().flatten().filter_map(|place| {
            Some((
                self.relations.get(*place)?,
                self.touches_tests.get(*place).copied().unwrap_or(true),
            ))
        })
    }
}

/// A symbol and everything inside it, at any depth, whether visible or not.
#[derive(Debug, Clone)]
pub struct Subtree<'a> {
    model: &'a ProjectModel,
    /// Whether the symbol at each place is inside.
    inside: Vec<bool>,
}

impl Subtree<'_> {
    /// Whether a symbol is the one the subtree is of, or inside it.
    #[must_use]
    pub fn contains(&self, id: &SymbolId) -> bool {
        self.model.place(id).is_some_and(|place| self.holds(place))
    }

    fn holds(&self, place: usize) -> bool {
        self.inside.get(place).copied().unwrap_or(false)
    }
}

/// The model as an analysis sees it, with or without test code.
#[derive(Debug, Clone, Copy)]
pub struct ModelView<'a> {
    model: &'a ProjectModel,
    tests: TestCode,
}

impl<'a> ModelView<'a> {
    #[must_use]
    pub const fn new(model: &'a ProjectModel, tests: TestCode) -> Self {
        Self { model, tests }
    }

    #[must_use]
    pub fn symbol(&self, id: &SymbolId) -> Option<&'a Symbol> {
        self.model
            .symbol(id)
            .filter(|symbol| self.tests.shows(symbol.role))
    }

    pub fn symbols(&self) -> impl Iterator<Item = &'a Symbol> {
        let tests = self.tests;
        self.model
            .symbols()
            .filter(move |symbol| tests.shows(symbol.role))
    }

    pub fn children(&self, id: &SymbolId) -> impl Iterator<Item = &'a Symbol> {
        let tests = self.tests;
        self.model
            .children(id)
            .filter(move |symbol| tests.shows(symbol.role))
    }

    /// The relations between visible symbols.
    pub fn relations(&self) -> impl Iterator<Item = &'a Relation> {
        let tests = self.tests;
        self.model
            .relations
            .iter()
            .zip(&self.model.touches_tests)
            .filter(move |(_, touches)| tests == TestCode::Included || !**touches)
            .map(|(relation, _)| relation)
    }

    /// The relations between visible symbols that start at a symbol, in order of where
    /// they end.
    pub fn relations_from(&self, id: &SymbolId) -> impl Iterator<Item = &'a Relation> {
        let tests = self.tests;
        self.model
            .relations_at(self.model.outgoing.get(id))
            .filter(move |(_, touches)| tests == TestCode::Included || !*touches)
            .map(|(relation, _)| relation)
    }

    /// The relations between visible symbols that end at a symbol, in order of where
    /// they start.
    pub fn relations_to(&self, id: &SymbolId) -> impl Iterator<Item = &'a Relation> {
        let tests = self.tests;
        self.model
            .relations_at(self.model.incoming.get(id))
            .filter(move |(_, touches)| tests == TestCode::Included || !*touches)
            .map(|(relation, _)| relation)
    }

    /// A symbol and everything inside it. Ask it rather than `is_within` when asking of
    /// many symbols.
    #[must_use]
    pub fn subtree(&self, id: &SymbolId) -> Subtree<'a> {
        let model = self.model;
        let mut inside = vec![false; model.symbols.len()];
        let mut pending: Vec<usize> = model.place(id).into_iter().collect();
        while let Some(place) = pending.pop() {
            match inside.get_mut(place) {
                Some(seen) if !*seen => *seen = true,
                _ => continue,
            }
            if let Some(children) = model.children_at.get(place) {
                pending.extend(children);
            }
        }
        Subtree { model, inside }
    }

    /// The relations between visible symbols with either end inside a subtree, in order
    /// of where they start.
    pub fn relations_touching<'s>(
        &self,
        subtree: &'s Subtree<'a>,
    ) -> impl Iterator<Item = &'a Relation> + 's
    where
        'a: 's,
    {
        let tests = self.tests;
        let model = self.model;
        model
            .relations
            .iter()
            .zip(&model.touches_tests)
            .zip(&model.ends)
            .filter(move |((_, touches), (from, to))| {
                (tests == TestCode::Included || !**touches)
                    && (subtree.holds(*from) || subtree.holds(*to))
            })
            .map(|((relation, _), _)| relation)
    }

    /// Whether a symbol is another one, or anywhere inside it.
    #[must_use]
    pub fn is_within(&self, id: &SymbolId, ancestor: &SymbolId) -> bool {
        let mut current = Some(id);
        while let Some(candidate) = current {
            if candidate == ancestor {
                return true;
            }
            current = self
                .model
                .symbol(candidate)
                .and_then(|symbol| symbol.parent.as_ref());
        }
        false
    }

    /// The module or package a symbol is in, or the symbol itself if it is one.
    #[must_use]
    pub fn module_of(&self, id: &SymbolId) -> Option<&'a Symbol> {
        let mut current = self.model.symbol(id);
        while let Some(symbol) = current {
            if matches!(symbol.kind, SymbolKind::Module | SymbolKind::Package) {
                return Some(symbol);
            }
            current = symbol
                .parent
                .as_ref()
                .and_then(|parent| self.model.symbol(parent));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(id: &str, parent: Option<&str>, role: CodeRole) -> Symbol {
        Symbol::container(
            SymbolId::new(id),
            parent.map(SymbolId::new),
            id,
            SymbolKind::Module,
            role,
        )
    }

    #[test]
    fn the_first_description_of_a_symbol_stays() {
        let model = ProjectModel::new(
            vec![
                symbol("shop", None, CodeRole::Production),
                symbol("shop", None, CodeRole::Test),
            ],
            Vec::new(),
        );
        let id = SymbolId::new("shop");
        assert_eq!(
            model.symbol(&id).map(|symbol| symbol.role),
            Some(CodeRole::Production)
        );
    }

    fn relation(from: &str, to: &str) -> Relation {
        Relation {
            from: SymbolId::new(from),
            to: SymbolId::new(to),
            kind: RelationKind::Imports,
            confidence: crate::relation::Confidence::Exact,
        }
    }

    #[test]
    fn the_relations_of_a_subtree_are_those_with_an_end_inside_it() {
        let model = ProjectModel::new(
            vec![
                symbol("shop", None, CodeRole::Production),
                symbol("shop::a", Some("shop"), CodeRole::Production),
                symbol("shop::a::x", Some("shop::a"), CodeRole::Production),
                symbol("shop::b", Some("shop"), CodeRole::Production),
                symbol("shop::tests", Some("shop"), CodeRole::Test),
            ],
            vec![
                relation("shop::a::x", "shop::b"),
                relation("shop::b", "shop::a"),
                relation("shop::b", "shop::tests"),
                relation("shop::tests", "shop::a::x"),
            ],
        );
        let ends = |view: ModelView<'_>, id: &str| -> Vec<(String, String)> {
            let subtree = view.subtree(&SymbolId::new(id));
            view.relations_touching(&subtree)
                .map(|relation| (relation.from.to_string(), relation.to.to_string()))
                .collect()
        };
        let pair = |from: &str, to: &str| (from.to_owned(), to.to_owned());
        let hidden = ModelView::new(&model, TestCode::Excluded);
        let shown = ModelView::new(&model, TestCode::Included);

        assert_eq!(
            ends(hidden, "shop::a"),
            vec![pair("shop::a::x", "shop::b"), pair("shop::b", "shop::a")]
        );
        assert_eq!(
            ends(shown, "shop::a"),
            vec![
                pair("shop::a::x", "shop::b"),
                pair("shop::b", "shop::a"),
                pair("shop::tests", "shop::a::x"),
            ]
        );
        assert_eq!(
            ends(hidden, "shop::a::x"),
            vec![pair("shop::a::x", "shop::b")]
        );
        assert_eq!(ends(hidden, "nowhere"), Vec::new());
        let subtree = hidden.subtree(&SymbolId::new("shop::a"));
        assert!(subtree.contains(&SymbolId::new("shop::a::x")));
        assert!(!subtree.contains(&SymbolId::new("shop::b")));
    }

    #[test]
    fn a_view_without_tests_hides_test_code_everywhere() {
        let model = ProjectModel::new(
            vec![
                symbol("shop", None, CodeRole::Production),
                symbol("shop::a", Some("shop"), CodeRole::Production),
                symbol("shop::tests", Some("shop"), CodeRole::Test),
            ],
            Vec::new(),
        );
        let shop = SymbolId::new("shop");
        let hidden = ModelView::new(&model, TestCode::Excluded);
        let shown = ModelView::new(&model, TestCode::Included);

        assert!(hidden.symbol(&SymbolId::new("shop::tests")).is_none());
        assert_eq!(hidden.children(&shop).count(), 1);
        assert_eq!(hidden.symbols().count(), 2);
        assert_eq!(shown.children(&shop).count(), 2);
        assert_eq!(shown.symbols().count(), 3);
    }
}
