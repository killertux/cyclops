//! Places the facts of every Python file in the project, and works out what their names
//! mean. A file is a module, a folder is the module that holds them, and `__init__.py`
//! is the module of its folder. A module is named after its path, as TypeScript's are.
//!
//! An import starts from the file's folder when it is relative, and otherwise from the
//! package's root or its `src` folder. Python names a module's functions and classes
//! exactly through its imports. A method is called on something whose type the syntax
//! does not say, so a method call is matched by name, as in TypeScript.

use cyclops_application::ports::{FileSource, Linked};
use cyclops_domain::facts::{FileFacts, Import, ReferenceKind, SymbolFact};
use cyclops_domain::path::RelativePath;
use cyclops_domain::relation::{Confidence, Relation, RelationKind};
use cyclops_domain::symbol::{SymbolId, SymbolKind};
use cyclops_domain::symbol_set::SymbolSet;
use std::collections::{HashMap, HashSet};

/// The manifest of a Python project.
const MANIFEST: &str = "pyproject.toml";

/// How far to follow a name an `__init__.py` passes on, so that a loop of them ends.
const MAX_REEXPORTS: usize = 8;

/// A package, or the project itself when there is no `pyproject.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Package {
    name: String,
    /// Its folder, relative to the project. Empty for the project root.
    root: String,
}

/// The name of a `pyproject.toml`: `[project]`'s, or else Poetry's.
fn package_name(manifest: &str) -> Option<String> {
    let table: toml::Table = manifest.parse().ok()?;
    let named =
        |section: Option<&toml::Value>| section?.get("name")?.as_str().map(ToOwned::to_owned);
    named(table.get("project"))
        .or_else(|| named(table.get("tool").and_then(|tool| tool.get("poetry"))))
}

fn parent_folder(path: &str) -> Option<&str> {
    if path.is_empty() {
        None
    } else {
        Some(path.rsplit_once('/').map_or("", |(folder, _)| folder))
    }
}

fn last_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn join(folder: &str, name: &str) -> String {
    match (folder.is_empty(), name.is_empty()) {
        (true, _) => name.to_owned(),
        (_, true) => folder.to_owned(),
        _ => format!("{folder}/{name}"),
    }
}

/// The path of the module a file is.
fn module_path(file: &str) -> &str {
    let without_extension = file
        .strip_suffix(".pyi")
        .or_else(|| file.strip_suffix(".py"))
        .unwrap_or(file);
    match without_extension.rsplit_once('/') {
        Some((folder, "__init__")) => folder,
        None if without_extension == "__init__" => "",
        _ => without_extension,
    }
}

struct Packages<'a> {
    source: &'a dyn FileSource,
    /// What each folder's manifest says, read once.
    names: HashMap<String, Option<String>>,
}

impl Packages<'_> {
    fn name_in(&mut self, folder: &str) -> Option<String> {
        if let Some(known) = self.names.get(folder) {
            return known.clone();
        }
        let name = self
            .source
            .read(&RelativePath::new(&join(folder, MANIFEST)))
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|text| package_name(&text));
        self.names.insert(folder.to_owned(), name.clone());
        name
    }

    /// The nearest package around a file.
    fn of(&mut self, path: &str) -> Package {
        let mut folder = parent_folder(path);
        while let Some(current) = folder {
            if let Some(name) = self.name_in(current) {
                return Package {
                    name,
                    root: current.to_owned(),
                };
            }
            folder = parent_folder(current);
        }
        Package {
            name: self.source.project_name(),
            root: String::new(),
        }
    }
}

/// A file, with its package, the module it is, and the ids its symbols got.
struct Placed<'a> {
    facts: &'a FileFacts,
    package: Package,
    module: SymbolId,
    ids: Vec<SymbolId>,
}

pub(crate) fn link(facts: &[&FileFacts], source: &dyn FileSource) -> Linked {
    let mut sorted = facts.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut packages = Packages {
        source,
        names: HashMap::new(),
    };
    let mut symbols = SymbolSet::default();
    let mut placed = Vec::with_capacity(sorted.len());
    for file in sorted {
        let package = packages.of(file.path.as_str());
        let module = place(&mut symbols, file, &package);
        let ids = add_symbols(&mut symbols, file, &module);
        placed.push(Placed {
            facts: file,
            package,
            module,
            ids,
        });
    }
    let relations = Resolver::new(&placed, &symbols).relations();
    let file_modules = placed
        .iter()
        .map(|file| (file.facts.path.clone(), file.module.clone()))
        .collect();
    Linked {
        symbols: symbols.into_symbols(),
        relations,
        test_files: Vec::new(),
        file_modules,
    }
}

/// Makes sure a file's module and the folders around it exist, and says which it is.
fn place(symbols: &mut SymbolSet, file: &FileFacts, package: &Package) -> SymbolId {
    let root = SymbolId::new(&package.name);
    let role = file.role;
    symbols.container(&root, None, &package.name, SymbolKind::Package, role);
    let path = module_path(file.path.as_str());
    let mut folders = Vec::new();
    let mut current = path;
    while current != package.root && !current.is_empty() {
        folders.push(current);
        current = parent_folder(current).unwrap_or_default();
    }
    let mut parent = root;
    for folder in folders.into_iter().rev() {
        let id = SymbolId::new(folder);
        symbols.container(
            &id,
            Some(&parent),
            last_name(folder),
            SymbolKind::Module,
            role,
        );
        parent = id;
    }
    symbols.written_in(&parent, file);
    parent
}

fn add_symbols(symbols: &mut SymbolSet, file: &FileFacts, module: &SymbolId) -> Vec<SymbolId> {
    let mut ids: Vec<SymbolId> = Vec::with_capacity(file.symbols.len());
    for fact in &file.symbols {
        let parent = fact
            .parent
            .and_then(|index| ids.get(index))
            .unwrap_or(module)
            .clone();
        let id = symbols.add(fact, parent.child(&fact.name), parent, &file.path);
        ids.push(id);
    }
    ids
}

/// What a name means: a symbol, or a whole module.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Meaning {
    Symbol(SymbolId),
    Module(usize),
}

struct Resolver<'a> {
    files: &'a [Placed<'a>],
    by_path: HashMap<&'a str, usize>,
    symbols: &'a SymbolSet,
    types_by_name: HashMap<&'a str, Vec<SymbolId>>,
    methods_by_name: HashMap<&'a str, Vec<SymbolId>>,
}

impl<'a> Resolver<'a> {
    fn new(files: &'a [Placed<'a>], symbols: &'a SymbolSet) -> Self {
        let mut types_by_name: HashMap<&str, Vec<SymbolId>> = HashMap::new();
        let mut methods_by_name: HashMap<&str, Vec<SymbolId>> = HashMap::new();
        for file in files {
            for (fact, id) in file.facts.symbols.iter().zip(&file.ids) {
                if fact.kind.is_type() {
                    types_by_name
                        .entry(&fact.name)
                        .or_default()
                        .push(id.clone());
                }
                if fact.kind == SymbolKind::Method {
                    methods_by_name
                        .entry(&fact.name)
                        .or_default()
                        .push(id.clone());
                }
            }
        }
        Self {
            files,
            by_path: files
                .iter()
                .enumerate()
                .map(|(index, file)| (file.facts.path.as_str(), index))
                .collect(),
            symbols,
            types_by_name,
            methods_by_name,
        }
    }

    /// The file a module path stands for: a `.py` or `.pyi` file, or a folder's
    /// `__init__`.
    fn probe(&self, base: &str) -> Option<usize> {
        let candidates = [
            format!("{base}.py"),
            format!("{base}.pyi"),
            join(base, "__init__.py"),
            join(base, "__init__.pyi"),
        ];
        candidates
            .iter()
            .find_map(|candidate| self.by_path.get(candidate.as_str()).copied())
    }

    /// The path, without extension, an import's module stands for, from each place it
    /// may start: the file's folder when it is relative, or else the package's root,
    /// its `src` folder, or the project's root.
    fn bases(&self, file: usize, module: &str) -> Vec<String> {
        let Some(placed) = self.files.get(file) else {
            return Vec::new();
        };
        let dots = module
            .chars()
            .take_while(|character| *character == '.')
            .count();
        let rest = module.get(dots..).unwrap_or_default().replace('.', "/");
        if dots > 0 {
            let mut folder = parent_folder(placed.facts.path.as_str()).unwrap_or_default();
            for _ in 1..dots {
                folder = parent_folder(folder).unwrap_or_default();
            }
            return vec![join(folder, &rest)];
        }
        let root = placed.package.root.as_str();
        let mut bases = vec![join(root, &rest), join(&join(root, "src"), &rest)];
        if !root.is_empty() {
            bases.push(rest.clone());
            bases.push(join("src", &rest));
        }
        bases
    }

    fn module_file(&self, file: usize, module: &str) -> Option<usize> {
        self.bases(file, module)
            .iter()
            .find_map(|base| self.probe(base))
    }

    /// The module an import depends on: the module it names, or the module inside it
    /// that it imports, as `from . import billing` does.
    fn imported_module(&self, file: usize, import: &Import) -> Option<usize> {
        let module = import.from.as_deref()?;
        let submodule = import.path.first().and_then(|name| {
            self.bases(file, module)
                .iter()
                .find_map(|base| self.probe(&join(base, name)))
        });
        submodule.or_else(|| self.module_file(file, module))
    }

    /// Something a module defines at its top level.
    fn top_level(&self, file: usize, name: &str) -> Option<SymbolId> {
        let placed = self.files.get(file)?;
        placed
            .facts
            .symbols
            .iter()
            .zip(&placed.ids)
            .find(|(fact, _)| fact.parent.is_none() && fact.name == name)
            .map(|(_, id)| id.clone())
    }

    /// What a name a module offers means: something it defines, or something it imports
    /// and so passes on, as an `__init__.py` does.
    fn offered(&self, file: usize, name: &str, depth: usize) -> Option<Meaning> {
        if let Some(id) = self.top_level(file, name) {
            return Some(Meaning::Symbol(id));
        }
        if depth > MAX_REEXPORTS {
            return None;
        }
        self.name_in(file, name, depth + 1)
    }

    /// What a name written in a file means: something the file defines, or something it
    /// imports.
    fn name_in(&self, file: usize, name: &str, depth: usize) -> Option<Meaning> {
        if depth == 0
            && let Some(id) = self.top_level(file, name)
        {
            return Some(Meaning::Symbol(id));
        }
        let placed = self.files.get(file)?;
        placed.facts.imports.iter().find_map(|import| {
            if import.alias.as_deref() != Some(name) {
                return None;
            }
            let module = import.from.as_deref()?;
            match import.path.first() {
                None => self.module_file(file, module).map(Meaning::Module),
                Some(original) => {
                    let target = self.imported_module(file, import)?;
                    let is_submodule = self
                        .module_file(file, module)
                        .is_none_or(|parent| parent != target);
                    if is_submodule {
                        Some(Meaning::Module(target))
                    } else {
                        self.offered(target, original, depth)
                    }
                }
            }
        })
    }

    /// What a path such as `billing.Invoice` means exactly.
    fn exact(&self, file: usize, path: &[String]) -> Option<SymbolId> {
        let (first, rest) = path.split_first()?;
        let mut meaning = self.name_in(file, first, 0)?;
        for name in rest {
            meaning = match meaning {
                Meaning::Module(module) => self.offered(module, name, 0)?,
                Meaning::Symbol(id) => Meaning::Symbol(id.child(name)),
            };
        }
        match meaning {
            Meaning::Symbol(id) => self.symbols.get(&id).map(|_| id),
            Meaning::Module(_) => None,
        }
    }

    fn kind_of(&self, id: &SymbolId) -> Option<SymbolKind> {
        self.symbols.get(id).map(|symbol| symbol.kind)
    }

    /// What a type name means: exactly, or failing that, every type with that name.
    fn types_named(&self, file: usize, path: &[String]) -> Vec<(SymbolId, Confidence)> {
        if let Some(found) = self
            .exact(file, path)
            .filter(|id| self.kind_of(id).is_some_and(SymbolKind::is_type))
        {
            return vec![(found, Confidence::Exact)];
        }
        let Some(name) = path.last() else {
            return Vec::new();
        };
        let candidates = self
            .types_by_name
            .get(name.as_str())
            .cloned()
            .unwrap_or_default();
        let confidence = Confidence::of_candidates(candidates.len());
        candidates.into_iter().map(|id| (id, confidence)).collect()
    }

    /// The class around a symbol.
    fn owner_type(&self, id: &SymbolId) -> Option<SymbolId> {
        let mut current = self.symbols.get(id)?.parent.as_ref();
        while let Some(candidate) = current {
            let symbol = self.symbols.get(candidate)?;
            if symbol.kind.is_type() {
                return Some(candidate.clone());
            }
            current = symbol.parent.as_ref();
        }
        None
    }

    fn relations(&self) -> Vec<Relation> {
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        let mut relate = |from: SymbolId, to: SymbolId, kind: RelationKind, confidence| {
            if kind == RelationKind::Imports && from == to {
                return;
            }
            let relation = Relation {
                from,
                to,
                kind,
                confidence,
            };
            if seen.insert(relation.clone()) {
                found.push(relation);
            }
        };
        for (index, file) in self.files.iter().enumerate() {
            for import in &file.facts.imports {
                let target = self
                    .imported_module(index, import)
                    .and_then(|target| self.files.get(target));
                if let Some(target) = target {
                    relate(
                        file.module.clone(),
                        target.module.clone(),
                        RelationKind::Imports,
                        Confidence::Exact,
                    );
                }
            }
            for (fact, id) in file.facts.symbols.iter().zip(&file.ids) {
                for (from, to, kind, confidence) in self.references(index, fact, id) {
                    relate(from, to, kind, confidence);
                }
            }
        }
        found
    }

    /// The relations of what one symbol mentions.
    fn references(
        &self,
        file: usize,
        fact: &SymbolFact,
        id: &SymbolId,
    ) -> Vec<(SymbolId, SymbolId, RelationKind, Confidence)> {
        let mut found = Vec::new();
        for reference in &fact.references {
            let path = &reference.path;
            match reference.kind {
                ReferenceKind::Type => {
                    let (from, kind) = if fact.kind == SymbolKind::Field {
                        let owner = self.symbols.get(id).and_then(|field| field.parent.clone());
                        (owner, RelationKind::Composes)
                    } else {
                        (Some(id.clone()), RelationKind::Uses)
                    };
                    if let Some(from) = from {
                        for (target, confidence) in self.types_named(file, path) {
                            found.push((from.clone(), target, kind, confidence));
                        }
                    }
                }
                ReferenceKind::Inherits | ReferenceKind::Implements => {
                    for (target, confidence) in self.types_named(file, path) {
                        let kind = if self.kind_of(&target) == Some(SymbolKind::Interface) {
                            RelationKind::Implements
                        } else {
                            RelationKind::Inherits
                        };
                        found.push((id.clone(), target, kind, confidence));
                    }
                }
                ReferenceKind::Call => {
                    let Some(target) = self.exact(file, path) else {
                        continue;
                    };
                    let kind = match self.kind_of(&target) {
                        Some(SymbolKind::Function | SymbolKind::Method) => RelationKind::Calls,
                        Some(kind) if kind.is_type() => RelationKind::Uses,
                        _ => continue,
                    };
                    found.push((id.clone(), target, kind, Confidence::Exact));
                }
                ReferenceKind::SelfCall => {
                    let target = self.owner_type(id).and_then(|owner| {
                        let method = path.iter().fold(owner, |at, name| at.child(name));
                        (self.kind_of(&method) == Some(SymbolKind::Method)).then_some(method)
                    });
                    if let Some(target) = target {
                        found.push((id.clone(), target, RelationKind::Calls, Confidence::Exact));
                    }
                }
                ReferenceKind::MethodCall => {
                    let name = path.last().map(String::as_str).unwrap_or_default();
                    let candidates = self.methods_by_name.get(name).cloned().unwrap_or_default();
                    let confidence = Confidence::of_candidates(candidates.len());
                    for target in candidates {
                        found.push((id.clone(), target, RelationKind::Calls, confidence));
                    }
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract;
    use cyclops_application::ports::{Discovered, FileStamp, SourceError};
    use cyclops_domain::symbol::Symbol;

    struct Project(&'static [(&'static str, &'static str)]);

    impl FileSource for Project {
        fn project_name(&self) -> String {
            "folder".to_owned()
        }

        fn discover(&self) -> Result<Discovered, SourceError> {
            Ok(Discovered::default())
        }

        fn stamp(&self, path: &RelativePath) -> Result<FileStamp, SourceError> {
            Err(SourceError::Unreadable(path.clone()))
        }

        fn read(&self, path: &RelativePath) -> Result<Vec<u8>, SourceError> {
            self.0
                .iter()
                .find(|(file, _)| *file == path.as_str())
                .map(|(_, text)| text.as_bytes().to_vec())
                .ok_or_else(|| SourceError::Unreadable(path.clone()))
        }
    }

    const SHOP: Project = Project(&[("pyproject.toml", "[project]\nname = \"shop\"\n")]);

    fn linked(project: &Project, files: &[(&str, &str)]) -> Linked {
        let facts: Vec<FileFacts> = files
            .iter()
            .map(|(path, text)| extract(&RelativePath::new(path), text))
            .collect();
        let refs: Vec<&FileFacts> = facts.iter().collect();
        link(&refs, project)
    }

    fn relations(project: &Project, files: &[(&str, &str)]) -> Vec<String> {
        let mut found: Vec<String> = linked(project, files)
            .relations
            .iter()
            .map(|relation| {
                format!(
                    "{} {} {} {}",
                    relation.from,
                    relation.kind.name(),
                    relation.to,
                    relation.confidence.name()
                )
            })
            .collect();
        found.sort();
        found
    }

    fn parent_of<'a>(symbols: &'a [Symbol], id: &str) -> Option<&'a str> {
        symbols
            .iter()
            .find(|symbol| symbol.id.as_str() == id)
            .and_then(|symbol| symbol.parent.as_ref())
            .map(SymbolId::as_str)
    }

    #[test]
    fn a_package_is_named_by_its_manifest() {
        assert_eq!(
            package_name("[project]\nname = \"shop\"\n").as_deref(),
            Some("shop")
        );
        assert_eq!(
            package_name("[tool.poetry]\nname = \"old\"\n").as_deref(),
            Some("old")
        );
        assert_eq!(package_name("[project]\n"), None);
    }

    #[test]
    fn modules_are_named_after_their_paths() {
        assert_eq!(module_path("src/shop/orders.py"), "src/shop/orders");
        assert_eq!(module_path("src/shop/__init__.py"), "src/shop");
        assert_eq!(module_path("types.pyi"), "types");
        assert_eq!(module_path("__init__.py"), "");
    }

    #[test]
    fn an_init_file_is_its_folder() {
        let symbols = linked(
            &SHOP,
            &[
                ("src/shop/__init__.py", "VERSION = 1\n"),
                ("src/shop/orders.py", "def total(): pass\n"),
            ],
        )
        .symbols;
        assert_eq!(parent_of(&symbols, "src/shop::VERSION"), Some("src/shop"));
        assert_eq!(parent_of(&symbols, "src/shop/orders"), Some("src/shop"));
        assert_eq!(parent_of(&symbols, "src"), Some("shop"));
    }

    #[test]
    fn imports_start_from_the_folder_or_the_source_root() {
        let found = relations(
            &SHOP,
            &[
                (
                    "src/shop/orders.py",
                    "from shop.billing import Invoice\nfrom .catalog import Item\nfrom . import notes\nimport os\n",
                ),
                ("src/shop/billing.py", "class Invoice: pass\n"),
                ("src/shop/catalog.py", "class Item: pass\n"),
                ("src/shop/notes.py", "def greeting(): pass\n"),
            ],
        );
        assert_eq!(
            found,
            [
                "src/shop/orders imports src/shop/billing exact",
                "src/shop/orders imports src/shop/catalog exact",
                "src/shop/orders imports src/shop/notes exact",
            ]
        );
    }

    #[test]
    fn names_reach_what_they_mean_through_imports() {
        let found = relations(
            &SHOP,
            &[
                (
                    "src/shop/orders.py",
                    "from shop import billing\nfrom shop.catalog import Item, Priced\n\
                     class Order(Item):\n    invoices: list[billing.Invoice]\n\
                     \n    def add(self, item: Item):\n        billing.make(item)\n        self.total()\n        item.price()\n\
                     \n    def total(self): pass\n",
                ),
                (
                    "src/shop/billing.py",
                    "class Invoice: pass\ndef make(item): pass\n",
                ),
                (
                    "src/shop/catalog.py",
                    "from abc import ABC\nclass Priced(ABC):\n    def price(self): ...\nclass Item(Priced):\n    def price(self): return 1\n",
                ),
            ],
        );
        let wanted = [
            "src/shop/catalog::Item implements src/shop/catalog::Priced exact",
            "src/shop/orders::Order composes src/shop/billing::Invoice exact",
            "src/shop/orders::Order inherits src/shop/catalog::Item exact",
            "src/shop/orders::Order::add calls src/shop/billing::make exact",
            "src/shop/orders::Order::add calls src/shop/catalog::Item::price ambiguous",
            "src/shop/orders::Order::add calls src/shop/catalog::Priced::price ambiguous",
            "src/shop/orders::Order::add calls src/shop/orders::Order::total exact",
            "src/shop/orders::Order::add uses src/shop/catalog::Item exact",
        ];
        for line in wanted {
            assert!(found.contains(&line.to_owned()), "{line} in {found:#?}");
        }
    }
}
