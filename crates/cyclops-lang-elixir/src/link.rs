//! Places the facts of every Elixir file in the project. A module is named after itself,
//! not its file, and sits inside the module of its prefix, up to the package: `Shop.Order`
//! is inside `Shop`, even when nothing defines `Shop`.
//!
//! Elixir names the module of every call it makes outside its own module, so calls are
//! exact. A module depends on every module it aliases, imports, requires, uses, names in
//! a call or a struct, or implements a protocol of.

use crate::symbols::IMPORTED_NAMES;
use cyclops_application::ports::{FileSource, Linked};
use cyclops_domain::facts::{FileFacts, ReferenceKind, SymbolFact};
use cyclops_domain::path::RelativePath;
use cyclops_domain::relation::{Confidence, Relation, RelationKind};
use cyclops_domain::symbol::{CodeRole, SymbolId, SymbolKind};
use cyclops_domain::symbol_set::SymbolSet;
use std::collections::{HashMap, HashSet};

/// The manifest of a Mix project.
const MANIFEST: &str = "mix.exs";

/// The `app:` a `mix.exs` names, such as `shop` for `app: :shop`.
fn package_name(manifest: &str) -> Option<String> {
    let (_, after) = manifest.split_once("app:")?;
    let atom = after.trim_start().strip_prefix(':')?;
    let name: String = atom
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

fn parent_folder(path: &str) -> Option<&str> {
    if path.is_empty() {
        None
    } else {
        Some(path.rsplit_once('/').map_or("", |(folder, _)| folder))
    }
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

fn last_part(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
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

    /// The name of the nearest package around a file.
    fn of(&mut self, path: &str) -> String {
        let mut folder = parent_folder(path);
        while let Some(current) = folder {
            if let Some(name) = self.name_in(current) {
                return name;
            }
            folder = parent_folder(current);
        }
        self.source.project_name()
    }
}

/// The symbols of a project as they are placed.
struct Placing {
    symbols: SymbolSet,
    /// Every module that exists, by name.
    modules: HashSet<String>,
}

impl Placing {
    /// Makes sure every prefix of a module exists, and says which holds it.
    fn prefixes(&mut self, name: &str, package: &SymbolId, role: CodeRole) -> SymbolId {
        let parts: Vec<&str> = name.split('.').collect();
        let mut parent = package.clone();
        for length in 1..parts.len() {
            let prefix = parts.get(..length).unwrap_or_default().join(".");
            let id = SymbolId::new(&prefix);
            if self.modules.insert(prefix.clone()) {
                self.symbols.container(
                    &id,
                    Some(&parent),
                    last_part(&prefix),
                    SymbolKind::Module,
                    role,
                );
            }
            parent = id;
        }
        parent
    }

    fn kind_of(&self, id: &SymbolId) -> Option<SymbolKind> {
        self.symbols.get(id).map(|symbol| symbol.kind)
    }

    /// The module a symbol is in, or the symbol itself if it is one.
    fn module_of(&self, id: &SymbolId) -> Option<SymbolId> {
        let mut symbol = self.symbols.get(id)?;
        while symbol.kind != SymbolKind::Module {
            symbol = self.symbols.get(symbol.parent.as_ref()?)?;
        }
        Some(symbol.id.clone())
    }

    /// The struct of a module, if it has one.
    fn structure(&self, module: &str) -> Option<SymbolId> {
        let id = SymbolId::new(module).child(last_part(module));
        (self.kind_of(&id) == Some(SymbolKind::Struct)).then_some(id)
    }

    /// The interface of a protocol.
    fn interface(&self, module: &str) -> Option<SymbolId> {
        let id = SymbolId::new(module).child(last_part(module));
        (self.kind_of(&id) == Some(SymbolKind::Interface)).then_some(id)
    }

    /// A function of a module, or of the protocol it is.
    fn function(&self, module: &str, key: &str) -> Option<SymbolId> {
        let callable = |kind| matches!(kind, Some(SymbolKind::Function | SymbolKind::Method));
        let direct = SymbolId::new(module).child(key);
        if callable(self.kind_of(&direct)) {
            return Some(direct);
        }
        let through = self.interface(module)?.child(key);
        callable(self.kind_of(&through)).then_some(through)
    }
}

/// Where each fact of each file was placed.
type Ids = Vec<Vec<Option<SymbolId>>>;

pub(crate) fn link(facts: &[&FileFacts], source: &dyn FileSource) -> Linked {
    let mut sorted = facts.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut packages = Packages {
        source,
        names: HashMap::new(),
    };
    let mut placing = Placing {
        symbols: SymbolSet::default(),
        modules: HashSet::new(),
    };
    let package_ids: Vec<SymbolId> = sorted
        .iter()
        .map(|file| {
            let name = packages.of(file.path.as_str());
            let id = SymbolId::new(&name);
            placing
                .symbols
                .container(&id, None, &name, SymbolKind::Package, file.role);
            id
        })
        .collect();
    let mut ids: Ids = sorted
        .iter()
        .map(|file| vec![None; file.symbols.len()])
        .collect();
    placing.place_modules(&sorted, &package_ids, &mut ids);
    // What implementations add to types, which may be written anywhere, comes last.
    for implementations in [false, true] {
        placing.place_members(&sorted, &package_ids, &mut ids, implementations);
    }
    let relations = Resolver {
        files: &sorted,
        ids: &ids,
        placing: &placing,
    }
    .relations();
    let file_modules = sorted
        .iter()
        .zip(&ids)
        .zip(&package_ids)
        .map(|((file, file_ids), package)| {
            let module = file
                .symbols
                .iter()
                .zip(file_ids)
                .find(|(fact, _)| fact.kind == SymbolKind::Module)
                .and_then(|(_, id)| id.clone())
                .unwrap_or_else(|| package.clone());
            (file.path.clone(), module)
        })
        .collect();
    Linked {
        symbols: placing.symbols.into_symbols(),
        relations,
        test_files: Vec::new(),
        file_modules,
    }
}

fn record(ids: &mut Ids, file: usize, index: usize, id: SymbolId) {
    if let Some(slot) = ids.get_mut(file).and_then(|ids| ids.get_mut(index)) {
        *slot = Some(id);
    }
}

impl Placing {
    /// Adds every module, in order of name, so that each prefix comes before what it
    /// holds.
    fn place_modules(&mut self, files: &[&FileFacts], packages: &[SymbolId], ids: &mut Ids) {
        let mut modules: Vec<(&str, usize, usize)> = Vec::new();
        for (file_index, file) in files.iter().enumerate() {
            for (index, fact) in file.symbols.iter().enumerate() {
                if fact.kind == SymbolKind::Module {
                    modules.push((&fact.name, file_index, index));
                }
            }
        }
        modules.sort_by(|a, b| a.0.cmp(b.0).then(a.1.cmp(&b.1)));
        for (name, file_index, index) in modules {
            let (Some(file), Some(package)) = (files.get(file_index), packages.get(file_index))
            else {
                continue;
            };
            let Some(fact) = file.symbols.get(index) else {
                continue;
            };
            let parent = self.prefixes(name, package, fact.role);
            let id = self
                .symbols
                .add(fact, SymbolId::new(name), parent, &file.path);
            self.modules.insert(id.as_str().to_owned());
            record(ids, file_index, index, id);
        }
    }

    /// Adds what the modules hold: the members of their own, or only those an
    /// implementation adds to a type.
    fn place_members(
        &mut self,
        files: &[&FileFacts],
        packages: &[SymbolId],
        ids: &mut Ids,
        implementations: bool,
    ) {
        for (file_index, file) in files.iter().enumerate() {
            for (index, fact) in file.symbols.iter().enumerate() {
                if fact.kind == SymbolKind::Module || fact.owner.is_some() != implementations {
                    continue;
                }
                let file_ids = ids.get(file_index);
                let local = fact
                    .parent
                    .and_then(|parent| file_ids?.get(parent)?.clone())
                    .or_else(|| packages.get(file_index).cloned());
                let owner = fact.owner.as_ref().and_then(|owner| {
                    let module = owner.first()?;
                    self.structure(module)
                        .or_else(|| self.modules.contains(module).then(|| SymbolId::new(module)))
                });
                let Some(parent) = owner.or(local) else {
                    continue;
                };
                let id = self
                    .symbols
                    .add(fact, parent.child(&fact.name), parent, &file.path);
                record(ids, file_index, index, id);
            }
        }
    }
}

struct Resolver<'a> {
    files: &'a [&'a FileFacts],
    ids: &'a Ids,
    placing: &'a Placing,
}

impl Resolver<'_> {
    fn id(&self, file: usize, index: usize) -> Option<&SymbolId> {
        self.ids.get(file)?.get(index)?.as_ref()
    }

    fn relations(&self) -> Vec<Relation> {
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        let mut relate = |from: SymbolId, to: SymbolId, kind: RelationKind| {
            if kind == RelationKind::Imports && from == to {
                return;
            }
            let relation = Relation {
                from,
                to,
                kind,
                confidence: Confidence::Exact,
            };
            if seen.insert(relation.clone()) {
                found.push(relation);
            }
        };
        // What each module brings in with `import`, whose functions it calls by name.
        let mut imported: HashMap<SymbolId, Vec<&str>> = HashMap::new();
        for (file_index, file) in self.files.iter().enumerate() {
            for import in &file.imports {
                let module = import.scope.and_then(|scope| self.id(file_index, scope));
                let target = import
                    .from
                    .as_deref()
                    .filter(|target| self.placing.modules.contains(*target));
                let (Some(module), Some(target)) = (module, target) else {
                    continue;
                };
                if import.alias.as_deref() == Some(IMPORTED_NAMES) {
                    imported.entry(module.clone()).or_default().push(target);
                }
                relate(module.clone(), SymbolId::new(target), RelationKind::Imports);
            }
            for extension in &file.extensions {
                let target = extension.owner.first();
                let protocol = extension.implements.as_ref().and_then(|path| path.first());
                let (Some(target), Some(protocol)) = (target, protocol) else {
                    continue;
                };
                let holder = extension
                    .parent
                    .and_then(|parent| self.id(file_index, parent).cloned())
                    .or_else(|| {
                        self.placing
                            .modules
                            .contains(target)
                            .then(|| SymbolId::new(target))
                    });
                if let Some(holder) = holder
                    && self.placing.modules.contains(protocol)
                {
                    relate(holder, SymbolId::new(protocol), RelationKind::Imports);
                }
                let implementer = self.placing.structure(target).or_else(|| {
                    self.placing
                        .modules
                        .contains(target)
                        .then(|| SymbolId::new(target))
                });
                if let (Some(implementer), Some(interface)) =
                    (implementer, self.placing.interface(protocol))
                {
                    relate(implementer, interface, RelationKind::Implements);
                }
            }
        }
        for (file_index, file) in self.files.iter().enumerate() {
            for (index, fact) in file.symbols.iter().enumerate() {
                let Some(id) = self.id(file_index, index) else {
                    continue;
                };
                for (from, to, kind) in self.references(fact, id, &imported) {
                    relate(from, to, kind);
                }
            }
        }
        found
    }

    /// The relations of what one symbol mentions, and the modules that makes its module
    /// depend on.
    fn references(
        &self,
        fact: &SymbolFact,
        id: &SymbolId,
        imported: &HashMap<SymbolId, Vec<&str>>,
    ) -> Vec<(SymbolId, SymbolId, RelationKind)> {
        let mut found = Vec::new();
        let module = self.placing.module_of(id);
        for reference in &fact.references {
            let named = match reference.kind {
                ReferenceKind::Type => {
                    let Some(target) = reference.path.first() else {
                        continue;
                    };
                    if let Some(structure) = self.placing.structure(target) {
                        found.push((id.clone(), structure, RelationKind::Uses));
                    }
                    Some(target)
                }
                ReferenceKind::Call => match reference.path.as_slice() {
                    [target, key] => {
                        if let Some(function) = self.placing.function(target, key) {
                            found.push((id.clone(), function, RelationKind::Calls));
                        }
                        Some(target)
                    }
                    [key] => {
                        let local = module
                            .as_ref()
                            .and_then(|module| self.placing.function(module.as_str(), key));
                        let through_import = || {
                            let modules = imported.get(module.as_ref()?)?;
                            modules
                                .iter()
                                .find_map(|target| self.placing.function(target, key))
                        };
                        if let Some(function) = local.or_else(through_import) {
                            found.push((id.clone(), function, RelationKind::Calls));
                        }
                        None
                    }
                    _ => None,
                },
                ReferenceKind::MethodCall
                | ReferenceKind::SelfCall
                | ReferenceKind::Implements
                | ReferenceKind::Inherits => None,
            };
            if let (Some(module), Some(target)) = (&module, named)
                && self.placing.modules.contains(target)
            {
                found.push((module.clone(), SymbolId::new(target), RelationKind::Imports));
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

    const SHOP: Project = Project(&[(
        "mix.exs",
        "defmodule Shop.MixProject do\n  def project, do: [app: :shop, version: \"0.1.0\"]\nend\n",
    )]);

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

    #[test]
    fn a_package_is_named_by_its_app() {
        assert_eq!(
            package_name("[app: :shop_web, version: \"1\"]").as_deref(),
            Some("shop_web")
        );
        assert_eq!(package_name("[version: \"1\"]"), None);
    }

    #[test]
    fn modules_sit_inside_their_prefixes_up_to_the_package() {
        let symbols = linked(
            &SHOP,
            &[
                (
                    "lib/shop/order.ex",
                    "defmodule Shop.Order do\n  defstruct [:a]\nend\n",
                ),
                ("lib/shop.ex", "defmodule Shop do\nend\n"),
            ],
        )
        .symbols;
        assert_eq!(parent_of(&symbols, "Shop"), Some("shop"));
        assert_eq!(parent_of(&symbols, "Shop.Order"), Some("Shop"));
        assert_eq!(parent_of(&symbols, "Shop.Order::Order"), Some("Shop.Order"));
        let shops = symbols
            .iter()
            .filter(|s| s.id.as_str().starts_with("Shop#"))
            .count();
        assert_eq!(shops, 0, "a defined prefix is not made up again");
    }

    #[test]
    fn an_implementation_adds_methods_to_its_type_wherever_it_is() {
        let found = relations(
            &SHOP,
            &[
                (
                    "lib/priced.ex",
                    "defprotocol Shop.Priced do\n  def price(item)\nend\n",
                ),
                (
                    "lib/impl.ex",
                    "defimpl Shop.Priced, for: Shop.Item do\n  def price(item), do: item.cost\nend\n",
                ),
                (
                    "lib/item.ex",
                    "defmodule Shop.Item do\n  defstruct [:cost]\nend\n",
                ),
            ],
        );
        assert_eq!(
            found,
            [
                "Shop.Item imports Shop.Priced",
                "Shop.Item::Item implements Shop.Priced::Priced",
            ]
        );
        let symbols = linked(
            &SHOP,
            &[
                ("lib/impl.ex", "defimpl Shop.Priced, for: Shop.Item do\n  def price(item), do: item.cost\nend\n"),
                ("lib/item.ex", "defmodule Shop.Item do\n  defstruct [:cost]\nend\n"),
            ],
        )
        .symbols;
        assert_eq!(
            parent_of(&symbols, "Shop.Item::Item::price/1"),
            Some("Shop.Item::Item")
        );
    }

    #[test]
    fn calls_reach_functions_by_module_name_and_arity() {
        let found = relations(
            &SHOP,
            &[
                (
                    "lib/a.ex",
                    "defmodule A do\n  alias B.Thing\n  import C\n  def f(x), do: x |> Thing.g() |> h() |> f2()\n  def f2(x), do: x\nend\n",
                ),
                (
                    "lib/b.ex",
                    "defmodule B.Thing do\n  def g(x), do: x\n  def g(x, y), do: y\nend\n",
                ),
                ("lib/c.ex", "defmodule C do\n  def h(x), do: x\nend\n"),
            ],
        );
        assert_eq!(
            found,
            [
                "A imports B.Thing",
                "A imports C",
                "A::f/1 calls A::f2/1",
                "A::f/1 calls B.Thing::g/1",
                "A::f/1 calls C::h/1",
            ]
        );
    }
}
