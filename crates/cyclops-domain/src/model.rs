//! The project model: every symbol, after linking.

use crate::symbol::{CodeRole, Symbol, SymbolId};
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
}

impl ProjectModel {
    /// Builds the model. If two languages describe the same symbol, such as the package
    /// they share, the first one stays.
    #[must_use]
    pub fn new(symbols: Vec<Symbol>) -> Self {
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
        Self {
            symbols: by_id,
            children,
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::SymbolKind;

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
        let model = ProjectModel::new(vec![
            symbol("shop", None, CodeRole::Production),
            symbol("shop", None, CodeRole::Test),
        ]);
        let id = SymbolId::new("shop");
        assert_eq!(
            model.symbol(&id).map(|symbol| symbol.role),
            Some(CodeRole::Production)
        );
    }

    #[test]
    fn a_view_without_tests_hides_test_code_everywhere() {
        let model = ProjectModel::new(vec![
            symbol("shop", None, CodeRole::Production),
            symbol("shop::a", Some("shop"), CodeRole::Production),
            symbol("shop::tests", Some("shop"), CodeRole::Test),
        ]);
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
