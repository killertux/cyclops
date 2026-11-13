//! Places the facts of every Gleam file in the project. A file is a module, a folder is
//! the module that holds them, and a file next to a folder of the same name, such as
//! `src/shop.gleam` next to `src/shop/`, is the module of that folder. A module is named
//! after its path, as TypeScript's are.
//!
//! Gleam names everything it uses: a module is imported, and a call names its module or
//! a name imported from it. So every relation is exact, and a name that leads nowhere
//! belongs to the standard library or a dependency.

use cyclops_application::ports::{FileSource, Linked};
use cyclops_domain::facts::{FileFacts, ReferenceKind, SymbolFact};
use cyclops_domain::path::RelativePath;
use cyclops_domain::relation::{Confidence, Relation, RelationKind};
use cyclops_domain::symbol::{SymbolId, SymbolKind};
use cyclops_domain::symbol_set::SymbolSet;
use std::collections::{HashMap, HashSet};

/// The manifest of a Gleam package.
const MANIFEST: &str = "gleam.toml";

/// The folders of a package whose files are named from inside them.
const SOURCE_FOLDERS: [&str; 3] = ["src", "test", "dev"];

/// A package, or the project itself when there is no `gleam.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Package {
    name: String,
    /// Its folder, relative to the project. Empty for the project root.
    root: String,
}

/// The `name` of a `gleam.toml`.
fn package_name(manifest: &str) -> Option<String> {
    let table: toml::Table = manifest.parse().ok()?;
    table.get("name")?.as_str().map(ToOwned::to_owned)
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
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

/// The path of the module a file is.
fn module_path(file: &str) -> &str {
    file.strip_suffix(".gleam").unwrap_or(file)
}

/// The name Gleam gives a file's module, as an import writes it: its path inside the
/// package's `src`, `test` or `dev` folder.
fn gleam_name(file: &str, package: &Package) -> String {
    let inside = if package.root.is_empty() {
        file
    } else {
        file.strip_prefix(&package.root)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(file)
    };
    let inside = SOURCE_FOLDERS
        .iter()
        .find_map(|folder| inside.strip_prefix(folder)?.strip_prefix('/'))
        .unwrap_or(inside);
    module_path(inside).to_owned()
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

/// A file, with the module it is, the name an import gives it, and the ids its symbols
/// got.
struct Placed<'a> {
    facts: &'a FileFacts,
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
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for file in sorted {
        let package = packages.of(file.path.as_str());
        let module = place(&mut symbols, file, &package);
        let ids = add_symbols(&mut symbols, file, &module);
        // `src` comes before `test`, so a name both use is the product's.
        by_name
            .entry(gleam_name(file.path.as_str(), &package))
            .or_insert(placed.len());
        placed.push(Placed {
            facts: file,
            module,
            ids,
        });
    }
    let relations = Resolver {
        files: &placed,
        by_name,
        symbols: &symbols,
    }
    .relations();
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

struct Resolver<'a> {
    files: &'a [Placed<'a>],
    /// Each file, by the name an import gives its module.
    by_name: HashMap<String, usize>,
    symbols: &'a SymbolSet,
}

impl Resolver<'_> {
    /// Something a module defines at its top level.
    fn top_level(
        &self,
        file: usize,
        name: &str,
        wanted: fn(SymbolKind) -> bool,
    ) -> Option<SymbolId> {
        let placed = self.files.get(file)?;
        placed
            .facts
            .symbols
            .iter()
            .zip(&placed.ids)
            .find(|(fact, _)| fact.parent.is_none() && fact.name == name && wanted(fact.kind))
            .map(|(_, id)| id.clone())
    }

    /// The file of a module a file imports as `alias`.
    fn imported_module(&self, file: usize, alias: &str) -> Option<usize> {
        let placed = self.files.get(file)?;
        placed.facts.imports.iter().find_map(|import| {
            let is_module = import.path.is_empty() && import.alias.as_deref() == Some(alias);
            is_module
                .then(|| self.by_name.get(import.from.as_deref()?).copied())
                .flatten()
        })
    }

    /// The file and original name of a name a file imports by itself, as `alias`.
    fn imported_name(&self, file: usize, alias: &str) -> Option<(usize, &str)> {
        let placed = self.files.get(file)?;
        placed.facts.imports.iter().find_map(|import| {
            let original = import.path.first()?;
            if import.alias.as_deref() != Some(alias) {
                return None;
            }
            let target = self.by_name.get(import.from.as_deref()?).copied()?;
            Some((target, original.as_str()))
        })
    }

    /// What a name written in a file means: something it defines, something it imports
    /// by name, or something of a module it imports.
    fn resolve(
        &self,
        file: usize,
        path: &[String],
        wanted: fn(SymbolKind) -> bool,
    ) -> Option<SymbolId> {
        match path {
            [name] => self.top_level(file, name, wanted).or_else(|| {
                let (target, original) = self.imported_name(file, name)?;
                self.top_level(target, original, wanted)
            }),
            [alias, name] => {
                let target = self.imported_module(file, alias)?;
                self.top_level(target, name, wanted)
            }
            _ => None,
        }
    }

    fn relations(&self) -> Vec<Relation> {
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        let mut relate = |from: &SymbolId, to: SymbolId, kind: RelationKind| {
            let relation = Relation {
                from: from.clone(),
                to,
                kind,
                confidence: Confidence::Exact,
            };
            if seen.insert(relation.clone()) {
                found.push(relation);
            }
        };
        for (index, file) in self.files.iter().enumerate() {
            for import in file
                .facts
                .imports
                .iter()
                .filter(|import| import.path.is_empty())
            {
                let target = import
                    .from
                    .as_deref()
                    .and_then(|name| self.by_name.get(name))
                    .and_then(|target| self.files.get(*target));
                if let Some(target) = target
                    && target.module != file.module
                {
                    relate(&file.module, target.module.clone(), RelationKind::Imports);
                }
            }
            for (fact, id) in file.facts.symbols.iter().zip(&file.ids) {
                for (from, to, kind) in self.references(index, fact, id) {
                    relate(&from, to, kind);
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
    ) -> Vec<(SymbolId, SymbolId, RelationKind)> {
        let mut found = Vec::new();
        for reference in &fact.references {
            match reference.kind {
                ReferenceKind::Type => {
                    let Some(target) = self.resolve(file, &reference.path, SymbolKind::is_type)
                    else {
                        continue;
                    };
                    if fact.kind == SymbolKind::Field {
                        let owner = self.symbols.get(id).and_then(|field| field.parent.clone());
                        if let Some(owner) = owner {
                            found.push((owner, target, RelationKind::Composes));
                        }
                    } else {
                        found.push((id.clone(), target, RelationKind::Uses));
                    }
                }
                ReferenceKind::Call => {
                    let callable = |kind| kind == SymbolKind::Function;
                    if let Some(target) = self.resolve(file, &reference.path, callable) {
                        found.push((id.clone(), target, RelationKind::Calls));
                    }
                }
                ReferenceKind::MethodCall
                | ReferenceKind::SelfCall
                | ReferenceKind::Implements
                | ReferenceKind::Inherits => {}
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
            .map(|relation| format!("{} {} {}", relation.from, relation.kind.name(), relation.to))
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

    const SHOP: Project = Project(&[("gleam.toml", "name = \"shop\"\nversion = \"1.0.0\"\n")]);

    #[test]
    fn a_package_is_named_by_its_manifest() {
        assert_eq!(package_name("name = \"shop\"\n").as_deref(), Some("shop"));
        assert_eq!(package_name("version = \"1\"\n"), None);
        assert_eq!(package_name("not toml ["), None);
    }

    #[test]
    fn a_module_is_named_from_inside_its_source_folder() {
        let package = Package {
            name: "shop".to_owned(),
            root: "apps/shop".to_owned(),
        };
        assert_eq!(gleam_name("apps/shop/src/shop/a.gleam", &package), "shop/a");
        assert_eq!(
            gleam_name("apps/shop/test/a_test.gleam", &package),
            "a_test"
        );
    }

    #[test]
    fn a_file_beside_its_folder_is_that_folder() {
        let symbols = linked(
            &SHOP,
            &[
                ("src/shop.gleam", "pub fn main() { Nil }"),
                ("src/shop/orders.gleam", "pub fn total() { 0 }"),
            ],
        )
        .symbols;
        assert_eq!(parent_of(&symbols, "src/shop::main"), Some("src/shop"));
        assert_eq!(parent_of(&symbols, "src/shop/orders"), Some("src/shop"));
        assert_eq!(parent_of(&symbols, "src/shop"), Some("src"));
        assert_eq!(parent_of(&symbols, "src"), Some("shop"));
    }

    #[test]
    fn imports_calls_and_types_lead_to_their_modules() {
        let found = relations(
            &SHOP,
            &[
                (
                    "src/shop/orders.gleam",
                    "import gleam/list\nimport shop/billing.{type Invoice, total as sum}\n\
                     pub type Order { Order(invoices: List(Invoice)) }\n\
                     pub fn total(o: Order) { list.length(o.invoices) + billing.is_free(1) + sum(2) }\n",
                ),
                (
                    "src/shop/billing.gleam",
                    "pub type Invoice { Invoice(cost: Int) }\n\
                     pub fn total(i) { i }\npub fn is_free(i) { total(i) == 0 }\n",
                ),
            ],
        );
        assert_eq!(
            found,
            [
                "src/shop/billing::is_free calls src/shop/billing::total",
                "src/shop/orders imports src/shop/billing",
                "src/shop/orders::Order composes src/shop/billing::Invoice",
                "src/shop/orders::total calls src/shop/billing::is_free",
                "src/shop/orders::total calls src/shop/billing::total",
                "src/shop/orders::total uses src/shop/orders::Order",
            ]
        );
    }

    #[test]
    fn a_test_imports_the_product_s_module() {
        let found = relations(
            &SHOP,
            &[
                ("src/shop/orders.gleam", "pub fn new() { 0 }"),
                (
                    "test/orders_test.gleam",
                    "import shop/orders\npub fn new_test() { orders.new() }",
                ),
            ],
        );
        assert_eq!(
            found,
            [
                "test/orders_test imports src/shop/orders",
                "test/orders_test::new_test calls src/shop/orders::new",
            ]
        );
    }
}
