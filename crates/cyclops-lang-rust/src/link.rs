//! Places the facts of every Rust file in the project: gives each symbol its full name
//! and its parent, and moves the members of an `impl` to their type.

use crate::layout::{Layout, Package, Place};
use cyclops_application::ports::FileSource;
use cyclops_domain::facts::{FileFacts, Import, SymbolFact};
use cyclops_domain::symbol::{CodeRole, Location, Span, Symbol, SymbolId, SymbolIds, SymbolKind};
use std::collections::{HashMap, HashSet};

/// A file with its place in the project.
struct Placed<'a> {
    facts: &'a FileFacts,
    /// The root of the crate the file is in. It is what `crate::` means.
    crate_root: SymbolId,
    /// The module the file is.
    module: SymbolId,
    /// The final id of each of its symbols, once it is known.
    ids: Vec<Option<SymbolId>>,
}

#[derive(Default)]
struct Linker {
    ids: SymbolIds,
    symbols: Vec<Symbol>,
    /// Where each package and module is in `symbols`.
    containers: HashMap<SymbolId, usize>,
    types: HashSet<SymbolId>,
}

pub(crate) fn link(facts: &[&FileFacts], source: &dyn FileSource) -> Vec<Symbol> {
    let mut sorted = facts.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut layout = Layout::new(sorted.iter().map(|file| &file.path), source);

    let mut linker = Linker::default();
    let mut files: Vec<Placed<'_>> = sorted
        .into_iter()
        .map(|file| {
            let package = layout.package_of(&file.path);
            let place = layout.place_of(&file.path, &package);
            linker.place(file, &package, place)
        })
        .collect();
    for file in &mut files {
        linker.free_symbols(file);
    }
    for file in &mut files {
        linker.members(file);
    }
    linker.symbols
}

fn whole_file(file: &FileFacts) -> Location {
    let lines = file.lines;
    Location {
        file: file.path.clone(),
        span: Span {
            start_line: 1,
            end_line: lines.code + lines.comment + lines.blank,
        },
    }
}

impl Linker {
    /// Makes sure a package or a module exists.
    fn container(
        &mut self,
        id: &SymbolId,
        parent: Option<&SymbolId>,
        name: &str,
        kind: SymbolKind,
        role: CodeRole,
    ) -> usize {
        if let Some(index) = self.containers.get(id) {
            if let Some(symbol) = self.symbols.get_mut(*index)
                && role == CodeRole::Production
            {
                symbol.role = role;
            }
            return *index;
        }
        let unique = self.ids.unique(id.clone());
        let symbol = Symbol::container(unique, parent.cloned(), name, kind, role);
        self.symbols.push(symbol);
        let index = self.symbols.len() - 1;
        self.containers.insert(id.clone(), index);
        index
    }

    fn place<'a>(&mut self, file: &'a FileFacts, package: &Package, place: Place) -> Placed<'a> {
        let role = file.role;
        let root = SymbolId::new(&package.name);
        self.container(&root, None, &package.name, SymbolKind::Package, role);

        let (crate_root, module, written) = match place {
            Place::Root => (root.clone(), root, false),
            Place::OwnCrate(path) => {
                let module = SymbolId::new(&path);
                self.container(&module, Some(&root), &path, SymbolKind::Module, role);
                (module.clone(), module, true)
            }
            Place::Module(names) => {
                let mut parent = root.clone();
                for name in names {
                    let id = parent.child(&name);
                    self.container(&id, Some(&parent), &name, SymbolKind::Module, role);
                    parent = id;
                }
                (root, parent, true)
            }
        };
        if written
            && let Some(index) = self.containers.get(&module)
            && let Some(symbol) = self.symbols.get_mut(*index)
        {
            symbol.location = Some(whole_file(file));
            symbol.code_lines = file.lines.code;
        }
        Placed {
            facts: file,
            crate_root,
            module,
            ids: vec![None; file.symbols.len()],
        }
    }

    fn add(
        &mut self,
        file: &mut Placed<'_>,
        index: usize,
        fact: &SymbolFact,
        id: SymbolId,
        parent: SymbolId,
    ) {
        let id = self.ids.unique(id);
        if fact.kind.is_type() {
            self.types.insert(id.clone());
        }
        if let Some(slot) = file.ids.get_mut(index) {
            *slot = Some(id.clone());
        }
        let symbol = Symbol::from_fact(fact, id, Some(parent), &file.facts.path);
        self.symbols.push(symbol);
    }

    /// The id of the symbol around a fact, if it has one yet.
    fn parent_of(file: &Placed<'_>, fact: &SymbolFact) -> Option<SymbolId> {
        match fact.parent {
            None => Some(file.module.clone()),
            Some(index) => file.ids.get(index).cloned().flatten(),
        }
    }

    /// Everything whose place does not depend on knowing every type.
    fn free_symbols(&mut self, file: &mut Placed<'_>) {
        let facts = file.facts;
        for (index, fact) in facts.symbols.iter().enumerate() {
            if fact.owner.is_none()
                && let Some(parent) = Self::parent_of(file, fact)
            {
                self.add(file, index, fact, parent.child(&fact.name), parent);
            }
        }
    }

    /// The members of `impl` blocks, and whatever is written inside them. A member of a
    /// type outside the project stays in its module, under the type's name.
    fn members(&mut self, file: &mut Placed<'_>) {
        let facts = file.facts;
        for (index, fact) in facts.symbols.iter().enumerate() {
            if file.ids.get(index).is_some_and(Option::is_some) {
                continue;
            }
            let Some(scope) = Self::parent_of(file, fact) else {
                continue;
            };
            let Some(owner) = &fact.owner else {
                self.add(file, index, fact, scope.child(&fact.name), scope);
                continue;
            };
            if let Some(owner_id) = self.resolve(owner, file, &scope) {
                self.add(file, index, fact, owner_id.child(&fact.name), owner_id);
            } else {
                let written = owner.join("::");
                let id = scope.child(&written).child(&fact.name);
                self.add(file, index, fact, id, scope);
            }
        }
    }

    /// Finds the type a path refers to, the way a person reading the file would.
    fn resolve(&self, path: &[String], file: &Placed<'_>, scope: &SymbolId) -> Option<SymbolId> {
        self.in_scopes(path, file, scope)
            .or_else(|| self.absolute(path, file))
            .or_else(|| self.through_imports(path, file))
    }

    /// Looks in each scope around the code, from the inside out.
    fn in_scopes(&self, path: &[String], file: &Placed<'_>, scope: &SymbolId) -> Option<SymbolId> {
        let mut current = scope.clone();
        loop {
            let candidate = Self::join(&current, path);
            if self.types.contains(&candidate) {
                return Some(candidate);
            }
            if current == file.module {
                return None;
            }
            current = Self::parent_id(&current)?;
        }
    }

    fn through_imports(&self, path: &[String], file: &Placed<'_>) -> Option<SymbolId> {
        let (first, rest) = path.split_first()?;
        let imports = &file.facts.imports;
        let named = imports
            .iter()
            .filter(|import| import.alias.as_ref() == Some(first))
            .find_map(|import| self.absolute(&Self::extended(import, rest), file));
        named.or_else(|| {
            imports
                .iter()
                .filter(|import| import.alias.is_none())
                .find_map(|import| self.absolute(&Self::extended(import, path), file))
        })
    }

    fn extended(import: &Import, rest: &[String]) -> Vec<String> {
        let mut path = import.path.clone();
        path.extend_from_slice(rest);
        path
    }

    /// Follows a path that says where it starts from.
    fn absolute(&self, path: &[String], file: &Placed<'_>) -> Option<SymbolId> {
        let (first, rest) = path.split_first()?;
        let mut start = match first.as_str() {
            "crate" => file.crate_root.clone(),
            "self" => file.module.clone(),
            "super" => Self::parent_id(&file.module)?,
            _ => {
                let from_package = SymbolId::new(&path.join("::"));
                let from_module = Self::join(&file.module, path);
                return [from_package, from_module]
                    .into_iter()
                    .find(|id| self.types.contains(id));
            }
        };
        let mut rest = rest;
        while let Some((next, others)) = rest.split_first() {
            if next != "super" {
                break;
            }
            start = Self::parent_id(&start)?;
            rest = others;
        }
        let candidate = Self::join(&start, rest);
        self.types.contains(&candidate).then_some(candidate)
    }

    fn join(start: &SymbolId, path: &[String]) -> SymbolId {
        path.iter().fold(start.clone(), |id, name| id.child(name))
    }

    fn parent_id(id: &SymbolId) -> Option<SymbolId> {
        id.as_str()
            .rsplit_once("::")
            .map(|(parent, _)| SymbolId::new(parent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract;
    use cyclops_application::ports::{Discovered, SourceError};
    use cyclops_domain::path::RelativePath;

    struct NoManifest;

    impl FileSource for NoManifest {
        fn project_name(&self) -> String {
            "p".to_owned()
        }

        fn discover(&self) -> Result<Discovered, SourceError> {
            Ok(Discovered::default())
        }

        fn read(&self, path: &RelativePath) -> Result<Vec<u8>, SourceError> {
            Err(SourceError::Unreadable(path.clone()))
        }
    }

    fn linked(files: &[(&str, &str)]) -> Vec<Symbol> {
        let facts: Vec<FileFacts> = files
            .iter()
            .map(|(path, text)| extract(&RelativePath::new(path), text))
            .collect();
        let refs: Vec<&FileFacts> = facts.iter().collect();
        link(&refs, &NoManifest)
    }

    fn ids(symbols: &[Symbol]) -> Vec<&str> {
        let mut ids: Vec<&str> = symbols.iter().map(|symbol| symbol.id.as_str()).collect();
        ids.sort_unstable();
        ids
    }

    fn parent_of<'a>(symbols: &'a [Symbol], id: &str) -> Option<&'a str> {
        symbols
            .iter()
            .find(|symbol| symbol.id.as_str() == id)
            .and_then(|symbol| symbol.parent.as_ref())
            .map(SymbolId::as_str)
    }

    #[test]
    fn a_method_in_another_file_joins_its_type() {
        let symbols = linked(&[
            ("src/store.rs", "pub struct Store;"),
            (
                "src/reading.rs",
                "use crate::store::Store;\nimpl Store { fn x(&self) {} }",
            ),
        ]);
        assert_eq!(
            parent_of(&symbols, "p::store::Store::x"),
            Some("p::store::Store")
        );
    }

    #[test]
    fn an_impl_can_name_the_path_of_its_type() {
        let symbols = linked(&[
            ("src/store.rs", "pub struct Store;"),
            ("src/a/b.rs", "impl crate::store::Store { fn x(&self) {} }"),
            ("src/a.rs", "impl super::store::Store { fn y(&self) {} }"),
        ]);
        assert!(ids(&symbols).contains(&"p::store::Store::x"));
        assert!(ids(&symbols).contains(&"p::store::Store::y"));
    }

    #[test]
    fn an_impl_finds_its_type_through_a_glob_or_an_alias() {
        let symbols = linked(&[
            ("src/store.rs", "pub struct Store;"),
            (
                "src/a.rs",
                "use crate::store::*;\nimpl Store { fn x(&self) {} }",
            ),
            (
                "src/b.rs",
                "use crate::store::Store as S;\nimpl S { fn y(&self) {} }",
            ),
        ]);
        assert!(ids(&symbols).contains(&"p::store::Store::x"));
        assert!(ids(&symbols).contains(&"p::store::Store::y"));
    }

    #[test]
    fn the_nearest_type_wins() {
        let symbols = linked(&[(
            "src/lib.rs",
            "struct S;\nmod m {\n    struct S;\n    impl S { fn x(&self) {} }\n}\n",
        )]);
        assert_eq!(parent_of(&symbols, "p::m::S::x"), Some("p::m::S"));
    }

    #[test]
    fn a_method_of_a_foreign_type_stays_in_its_module() {
        let symbols = linked(&[("src/a.rs", "impl Foreign for Vec<u8> { fn x(&self) {} }")]);
        assert_eq!(parent_of(&symbols, "p::a::Vec::x"), Some("p::a"));
    }

    #[test]
    fn two_symbols_with_one_name_stay_apart() {
        let symbols = linked(&[(
            "src/lib.rs",
            "struct S;\nimpl A for S { fn fmt(&self) {} }\nimpl B for S { fn fmt(&self) {} }\n",
        )]);
        assert_eq!(ids(&symbols), ["p", "p::S", "p::S::fmt", "p::S::fmt#2"]);
    }

    #[test]
    fn what_is_written_inside_a_method_follows_it() {
        let symbols = linked(&[
            ("src/store.rs", "pub struct Store;"),
            (
                "src/a.rs",
                "use crate::store::Store;\nimpl Store { fn x(&self) { fn inner() {} } }",
            ),
        ]);
        assert_eq!(
            parent_of(&symbols, "p::store::Store::x::inner"),
            Some("p::store::Store::x")
        );
    }

    #[test]
    fn a_folder_with_no_file_is_still_a_module() {
        let symbols = linked(&[("src/a/b.rs", "fn f() {}")]);
        assert_eq!(ids(&symbols), ["p", "p::a", "p::a::b", "p::a::b::f"]);
        assert_eq!(parent_of(&symbols, "p::a"), Some("p"));
    }

    #[test]
    fn a_module_file_after_its_children_still_describes_the_module() {
        let symbols = linked(&[("src/a/b.rs", ""), ("src/a/mod.rs", "// c\nfn f() {}\n")]);
        let module = symbols
            .iter()
            .find(|symbol| symbol.id.as_str() == "p::a")
            .expect("the module exists");
        let location = module.location.as_ref().expect("it has a file");
        assert_eq!(location.file.as_str(), "src/a/mod.rs");
    }

    #[test]
    fn a_test_file_does_not_make_its_package_test_code() {
        let symbols = linked(&[("src/lib.rs", ""), ("tests/a.rs", "fn f() {}")]);
        let role = |id: &str| {
            symbols
                .iter()
                .find(|symbol| symbol.id.as_str() == id)
                .map(|symbol| symbol.role)
        };
        assert_eq!(role("p"), Some(CodeRole::Production));
        assert_eq!(role("p::tests::a"), Some(CodeRole::Test));
        assert_eq!(role("p::tests::a::f"), Some(CodeRole::Test));
    }
}
