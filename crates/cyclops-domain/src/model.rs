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
    symbols: BTreeMap<SymbolId, Symbol>,
    children: HashMap<SymbolId, Vec<SymbolId>>,
    relations: Vec<Relation>,
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
        let mut children: HashMap<SymbolId, Vec<SymbolId>> = HashMap::new();
        for symbol in by_id.values() {
            if let Some(parent) = &symbol.parent {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(symbol.id.clone());
            }
        }
        let mut best: BTreeMap<(SymbolId, SymbolId, RelationKind), Relation> = BTreeMap::new();
        for relation in relations {
            if relation.from == relation.to && relation.kind != RelationKind::Calls {
                continue;
            }
            if !by_id.contains_key(&relation.from) || !by_id.contains_key(&relation.to) {
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
        Self {
            symbols: by_id,
            children,
            relations: best.into_values().collect(),
        }
    }

    /// Every relation, in order of where it starts.
    pub fn relations(&self) -> impl Iterator<Item = &Relation> {
        self.relations.iter()
    }

    #[must_use]
    pub fn symbol(&self, id: &SymbolId) -> Option<&Symbol> {
        self.symbols.get(id)
    }

    /// Every symbol, in order of id.
    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.values()
    }

    /// What is directly inside a symbol, in order of id.
    pub fn children(&self, id: &SymbolId) -> impl Iterator<Item = &Symbol> {
        self.children
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|child| self.symbols.get(child))
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
        let view = *self;
        self.model.relations().filter(move |relation| {
            view.symbol(&relation.from).is_some() && view.symbol(&relation.to).is_some()
        })
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
