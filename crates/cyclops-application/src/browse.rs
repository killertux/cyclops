//! What an interface needs to browse a project: the hierarchy in order of a metric,
//! finding a symbol by name, and where to look inside a symbol. See `specs/tui/`.

use crate::analysis::ProjectAnalysis;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::symbol::{Symbol, SymbolId, SymbolKind};
use std::cmp::Ordering;

/// Every metric is better low, or neutral, so the worst comes first by the highest value.
fn worst_first(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.total_cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

const fn is_function(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::Function | SymbolKind::Method)
}

impl ProjectAnalysis {
    /// Symbols in order of a metric, worst first. Ties are alphabetical, and symbols
    /// with no value go last.
    fn ordered_by<'a>(&self, symbols: Vec<&'a Symbol>, metric: MetricId) -> Vec<&'a Symbol> {
        let mut valued: Vec<(&Symbol, Option<f64>)> = symbols
            .into_iter()
            .map(|symbol| (symbol, self.symbol_value(&symbol.id, metric)))
            .collect();
        valued.sort_by(|(a, x), (b, y)| {
            worst_first(*x, *y)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.id.cmp(&b.id))
        });
        valued.into_iter().map(|(symbol, _)| symbol).collect()
    }

    /// The visible symbols at the top of the hierarchy, worst first by a metric.
    #[must_use]
    pub fn roots_by(&self, metric: MetricId) -> Vec<&Symbol> {
        let view = self.view();
        let ids = self.memo().roots.get_or_init(|| {
            view.symbols()
                .filter(|symbol| {
                    symbol
                        .parent
                        .as_ref()
                        .is_none_or(|parent| view.symbol(parent).is_none())
                })
                .map(|symbol| symbol.id.clone())
                .collect()
        });
        let roots = ids.iter().filter_map(|id| view.symbol(id)).collect();
        self.ordered_by(roots, metric)
    }

    /// What is visible directly inside a symbol, worst first by a metric.
    #[must_use]
    pub fn children_by(&self, id: &SymbolId, metric: MetricId) -> Vec<&Symbol> {
        self.ordered_by(self.view().children(id).collect(), metric)
    }

    /// The visible symbols whose name matches what was typed, the best first: an exact
    /// name or a whole ending of the id, such as `Order::total`, and then any symbol
    /// whose id contains it.
    #[must_use]
    pub fn find_by_name(&self, typed: &str) -> Vec<&Symbol> {
        let typed = typed.trim();
        if typed.is_empty() {
            return Vec::new();
        }
        let ending = format!("::{typed}");
        let lower = typed.to_lowercase();
        let mut exact = Vec::new();
        let mut partial = Vec::new();
        for symbol in self.view().symbols() {
            let id = symbol.id.as_str();
            if symbol.name == typed || id == typed || id.ends_with(&ending) {
                exact.push(symbol);
            } else if id.to_lowercase().contains(&lower) {
                partial.push(symbol);
            }
        }
        let shortest = |a: &&Symbol, b: &&Symbol| {
            a.id.as_str()
                .len()
                .cmp(&b.id.as_str().len())
                .then_with(|| a.id.cmp(&b.id))
        };
        exact.sort_by(shortest);
        partial.sort_by(shortest);
        exact.extend(partial);
        exact
    }

    /// The worst function inside a symbol by a metric, or the symbol itself if it is a
    /// function. A metric measured on files, such as `hotspot`, looks in its worst file.
    /// A metric that functions do not have falls back to `cyclomatic`.
    #[must_use]
    pub fn worst_function_in(&self, id: &SymbolId, metric: MetricId) -> Option<&Symbol> {
        let view = self.view();
        let symbol = view.symbol(id)?;
        if is_function(symbol.kind) {
            return Some(symbol);
        }
        let inside = |symbol: &&Symbol| is_function(symbol.kind) && view.is_within(&symbol.id, id);
        let mut functions: Vec<&Symbol> = view.symbols().filter(inside).collect();
        if matches!(metric, MetricId::Churn | MetricId::Hotspot) {
            let worst_file = self
                .files()
                .iter()
                .filter(|file| {
                    file.module
                        .as_ref()
                        .is_some_and(|module| view.is_within(module, id))
                })
                .filter_map(|file| {
                    let value = self.value(&Subject::File(file.path.clone()), metric)?;
                    Some((file, value))
                })
                .min_by(|(a, x), (b, y)| y.total_cmp(x).then_with(|| a.path.cmp(&b.path)))
                .map(|(file, _)| file.path.clone());
            if let Some(path) = worst_file {
                functions.retain(|function| {
                    function
                        .location
                        .as_ref()
                        .is_some_and(|location| location.file == path)
                });
            }
        }
        let measured = if functions
            .iter()
            .any(|function| self.symbol_value(&function.id, metric).is_some())
        {
            metric
        } else {
            MetricId::Cyclomatic
        };
        self.ordered_by(functions, measured).first().copied()
    }

    /// The symbol itself if it is visible, or else its nearest visible ancestor.
    #[must_use]
    pub fn nearest_visible(&self, id: &SymbolId) -> Option<&Symbol> {
        let view = self.view();
        let mut current = self.model().symbol(id);
        while let Some(symbol) = current {
            if let Some(visible) = view.symbol(&symbol.id) {
                return Some(visible);
            }
            current = symbol
                .parent
                .as_ref()
                .and_then(|parent| self.model().symbol(parent));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::{Extras, ProjectAnalysis};
    use cyclops_domain::metric::{Measurement, MetricId, Subject};
    use cyclops_domain::model::{ProjectModel, TestCode};
    use cyclops_domain::symbol::{CodeRole, Symbol, SymbolId, SymbolKind};

    fn symbol(id: &str, kind: SymbolKind, role: CodeRole) -> Symbol {
        let (parent, name) = id.rsplit_once("::").map_or((None, id), |(parent, name)| {
            (Some(SymbolId::new(parent)), name)
        });
        Symbol::container(SymbolId::new(id), parent, name, kind, role)
    }

    fn cyclomatic(id: &str, value: f64) -> Measurement {
        Measurement {
            metric: MetricId::Cyclomatic,
            subject: Subject::Symbol(SymbolId::new(id)),
            value,
        }
    }

    /// `shop` has `orders`, `billing` and `notes`, with a test module in `notes`.
    fn shop() -> ProjectAnalysis {
        let production = CodeRole::Production;
        let symbols = vec![
            symbol("shop", SymbolKind::Package, production),
            symbol("shop::orders", SymbolKind::Module, production),
            symbol("shop::orders::Order", SymbolKind::Struct, production),
            symbol("shop::orders::Order::total", SymbolKind::Method, production),
            symbol("shop::billing", SymbolKind::Module, production),
            symbol("shop::billing::total", SymbolKind::Function, production),
            symbol("shop::notes", SymbolKind::Module, production),
            symbol("shop::notes::tests", SymbolKind::Module, CodeRole::Test),
            symbol(
                "shop::notes::tests::says_hello",
                SymbolKind::Function,
                CodeRole::Test,
            ),
        ];
        let measurements = vec![
            cyclomatic("shop::orders", 3.0),
            cyclomatic("shop::orders::Order::total", 3.0),
            cyclomatic("shop::billing", 3.0),
            cyclomatic("shop::billing::total", 3.0),
        ];
        ProjectAnalysis::new(
            ProjectModel::new(symbols, Vec::new()),
            Vec::new(),
            measurements,
            Vec::new(),
            TestCode::Excluded,
            0,
            Extras::default(),
        )
    }

    fn ids(symbols: &[&Symbol]) -> Vec<String> {
        symbols.iter().map(|symbol| symbol.id.to_string()).collect()
    }

    #[test]
    fn children_go_worst_first_then_alphabetically_then_without_a_value() {
        let shop = shop();
        let children = shop.children_by(&SymbolId::new("shop"), MetricId::Cyclomatic);
        assert_eq!(
            ids(&children),
            vec!["shop::billing", "shop::orders", "shop::notes"]
        );
        assert_eq!(ids(&shop.roots_by(MetricId::Cyclomatic)), vec!["shop"]);
    }

    #[test]
    fn an_exact_name_or_ending_comes_before_a_part_of_one() {
        let shop = shop();
        assert_eq!(
            ids(&shop.find_by_name("total")),
            vec!["shop::billing::total", "shop::orders::Order::total"]
        );
        assert_eq!(
            ids(&shop.find_by_name("Order::total")),
            vec!["shop::orders::Order::total"]
        );
        assert_eq!(
            ids(&shop.find_by_name("ord")).first().map(String::as_str),
            Some("shop::orders")
        );
        assert_eq!(ids(&shop.find_by_name("says_hello")), Vec::<String>::new());
        assert_eq!(ids(&shop.find_by_name("  ")), Vec::<String>::new());
    }

    #[test]
    fn the_worst_function_inside_is_found_for_anything_larger() {
        let shop = shop();
        let worst = |id: &str| {
            shop.worst_function_in(&SymbolId::new(id), MetricId::Cyclomatic)
                .map(|symbol| symbol.id.to_string())
        };
        assert_eq!(worst("shop").as_deref(), Some("shop::billing::total"));
        assert_eq!(
            worst("shop::orders").as_deref(),
            Some("shop::orders::Order::total")
        );
        assert_eq!(
            worst("shop::billing::total").as_deref(),
            Some("shop::billing::total")
        );
        assert_eq!(worst("shop::notes"), None);
    }

    #[test]
    fn a_hidden_symbol_is_replaced_by_its_nearest_visible_ancestor() {
        let shop = shop();
        let nearest = shop
            .nearest_visible(&SymbolId::new("shop::notes::tests::says_hello"))
            .map(|symbol| symbol.id.to_string());
        assert_eq!(nearest.as_deref(), Some("shop::notes"));
        assert!(shop.nearest_visible(&SymbolId::new("nowhere")).is_none());
    }
}
