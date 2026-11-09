//! The project above its packages. See `specs/model/symbols.md`.

use cyclops_domain::symbol::{CodeRole, Symbol, SymbolId, SymbolKind};

/// Puts a project symbol above the packages when there are several of them. A project
/// of one package is that package, so nothing changes then.
pub fn add_project(symbols: &mut Vec<Symbol>, name: &str) {
    let packages: Vec<usize> = symbols
        .iter()
        .enumerate()
        .filter(|(_, symbol)| symbol.kind == SymbolKind::Package && symbol.parent.is_none())
        .map(|(index, _)| index)
        .collect();
    if packages.len() < 2 {
        return;
    }
    let plain = SymbolId::new(name);
    let id = if symbols.iter().any(|symbol| symbol.id == plain) {
        SymbolId::new(&format!("{name}#project"))
    } else {
        plain
    };
    let mut role = CodeRole::Test;
    for index in packages {
        if let Some(package) = symbols.get_mut(index) {
            package.parent = Some(id.clone());
            if package.role == CodeRole::Production {
                role = CodeRole::Production;
            }
        }
    }
    symbols.push(Symbol::container(id, None, name, SymbolKind::Project, role));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(id: &str, role: CodeRole) -> Symbol {
        Symbol::container(SymbolId::new(id), None, id, SymbolKind::Package, role)
    }

    fn parent_of<'a>(symbols: &'a [Symbol], id: &str) -> Option<&'a str> {
        symbols
            .iter()
            .find(|symbol| symbol.id == SymbolId::new(id))
            .and_then(|symbol| symbol.parent.as_ref())
            .map(SymbolId::as_str)
    }

    #[test]
    fn several_packages_go_inside_a_project_named_after_the_folder() {
        let mut symbols = vec![
            package("engine", CodeRole::Production),
            package("c01", CodeRole::Test),
        ];
        add_project(&mut symbols, "rust-crates");
        assert_eq!(parent_of(&symbols, "engine"), Some("rust-crates"));
        assert_eq!(parent_of(&symbols, "c01"), Some("rust-crates"));
        let project = symbols.last().expect("a project");
        assert_eq!(project.kind, SymbolKind::Project);
        assert_eq!(project.name, "rust-crates");
        assert_eq!(project.role, CodeRole::Production);
    }

    #[test]
    fn one_package_is_the_project() {
        let mut symbols = vec![package("shop", CodeRole::Production)];
        add_project(&mut symbols, "rust-shop");
        assert_eq!(symbols.len(), 1);
        assert_eq!(parent_of(&symbols, "shop"), None);
    }

    #[test]
    fn a_package_named_like_the_folder_keeps_its_name() {
        let mut symbols = vec![
            package("app", CodeRole::Production),
            package("tools", CodeRole::Production),
        ];
        add_project(&mut symbols, "app");
        assert_eq!(parent_of(&symbols, "app"), Some("app#project"));
    }
}
