//! Places the facts of every Rust file in the project: gives each symbol its full name
//! and its parent, moves the members of an `impl` to their type, and works out the
//! relations between symbols.

use crate::layout::{Layout, Package, Place};
use cyclops_application::ports::{FileSource, Linked};
use cyclops_domain::facts::{FileFacts, Import, ReferenceKind, SymbolFact};
use cyclops_domain::relation::{Confidence, Relation, RelationKind};
use cyclops_domain::symbol::{CodeRole, SymbolId, SymbolKind};
use cyclops_domain::symbol_set::SymbolSet;
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

/// Where a name is read, which decides what it can mean.
struct Context<'a> {
    file: &'a Placed<'a>,
    /// The innermost symbol around the name.
    local: SymbolId,
    /// The module around the name, which is what `self::` means.
    module: SymbolId,
}

/// What a name can be, for each kind of reference.
#[derive(Clone, Copy)]
enum Wanted {
    Type,
    Callable,
    Anything,
}

impl Wanted {
    const fn accepts(self, kind: SymbolKind) -> bool {
        match self {
            Self::Type => kind.is_type(),
            Self::Callable => matches!(kind, SymbolKind::Function | SymbolKind::Method),
            Self::Anything => true,
        }
    }
}

#[derive(Default)]
struct Linker {
    symbols: SymbolSet,
    /// The kind and the parent of every symbol.
    known: HashMap<SymbolId, (SymbolKind, Option<SymbolId>)>,
    types_by_name: HashMap<String, Vec<SymbolId>>,
    methods_by_name: HashMap<String, Vec<SymbolId>>,
    relations: Vec<Relation>,
}

pub(crate) fn link(facts: &[&FileFacts], source: &dyn FileSource) -> Linked {
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
    // Fields come last, so that a method that shares a field's name keeps that name.
    for file in &mut files {
        linker.fields(file);
    }
    for file in &files {
        linker.extensions(file);
        linker.references(file);
        linker.imports(file);
    }
    let test_modules = declared_test_modules(&files);
    let test_files = files
        .iter()
        .filter(|file| is_inside_any(&file.module, &test_modules))
        .map(|file| file.facts.path.clone())
        .collect();
    let file_modules = files
        .iter()
        .map(|file| (file.facts.path.clone(), file.module.clone()))
        .collect();
    let mut symbols = linker.symbols.into_symbols();
    for symbol in &mut symbols {
        if is_inside_any(&symbol.id, &test_modules) {
            symbol.role = CodeRole::Test;
        }
    }
    Linked {
        symbols,
        relations: linker.relations,
        test_files,
        file_modules,
    }
}

/// The modules some file declares to be test code, such as `#[cfg(test)] mod tests;`.
fn declared_test_modules(files: &[Placed<'_>]) -> HashSet<SymbolId> {
    files
        .iter()
        .flat_map(|file| {
            let module = &file.module;
            let declared = file.facts.test_modules.iter();
            declared.map(|path| join(module, path))
        })
        .collect()
}

/// Whether a symbol is one of some modules, or anywhere inside one.
fn is_inside_any(id: &SymbolId, modules: &HashSet<SymbolId>) -> bool {
    let mut current = Some(id.clone());
    while let Some(candidate) = current {
        if modules.contains(&candidate) {
            return true;
        }
        current = parent_id(&candidate);
    }
    false
}

fn join(start: &SymbolId, path: &[String]) -> SymbolId {
    path.iter().fold(start.clone(), |id, name| id.child(name))
}

fn parent_id(id: &SymbolId) -> Option<SymbolId> {
    id.as_str()
        .rsplit_once("::")
        .map(|(parent, _)| SymbolId::new(parent))
}

fn is_keyword(name: &str) -> bool {
    matches!(name, "crate" | "self" | "super" | "Self")
}

impl Linker {
    fn container(
        &mut self,
        id: &SymbolId,
        parent: Option<&SymbolId>,
        name: &str,
        kind: SymbolKind,
        role: CodeRole,
    ) {
        self.symbols.container(id, parent, name, kind, role);
        self.known
            .entry(id.clone())
            .or_insert_with(|| (kind, parent.cloned()));
    }

    fn place<'a>(&mut self, file: &'a FileFacts, package: &Package, place: Place) -> Placed<'a> {
        let role = file.role;
        let root = SymbolId::new(&package.name);
        self.container(&root, None, &package.name, SymbolKind::Package, role);

        let (crate_root, module) = match place {
            Place::Root => (root.clone(), root),
            Place::OwnCrate(path) => {
                let module = SymbolId::new(&path);
                self.container(&module, Some(&root), &path, SymbolKind::Module, role);
                (module.clone(), module)
            }
            Place::Module(names) => {
                let mut parent = root.clone();
                for name in names {
                    let id = parent.child(&name);
                    self.container(&id, Some(&parent), &name, SymbolKind::Module, role);
                    parent = id;
                }
                (root, parent)
            }
        };
        self.symbols.written_in(&module, file);
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
        let id = self.symbols.add(fact, id, parent.clone(), &file.facts.path);
        self.known.insert(id.clone(), (fact.kind, Some(parent)));
        if fact.kind.is_type() {
            self.types_by_name
                .entry(fact.name.clone())
                .or_default()
                .push(id.clone());
        }
        if fact.kind == SymbolKind::Method {
            self.methods_by_name
                .entry(fact.name.clone())
                .or_default()
                .push(id.clone());
        }
        if let Some(slot) = file.ids.get_mut(index) {
            *slot = Some(id);
        }
    }

    /// The id of the symbol around a fact, if it has one yet.
    fn parent_of(file: &Placed<'_>, parent: Option<usize>) -> Option<SymbolId> {
        match parent {
            None => Some(file.module.clone()),
            Some(index) => file.ids.get(index).cloned().flatten(),
        }
    }

    /// Everything whose place does not depend on knowing every type.
    fn free_symbols(&mut self, file: &mut Placed<'_>) {
        let facts = file.facts;
        for (index, fact) in facts.symbols.iter().enumerate() {
            if fact.owner.is_none()
                && fact.kind != SymbolKind::Field
                && let Some(parent) = Self::parent_of(file, fact.parent)
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
            if fact.kind == SymbolKind::Field || file.ids.get(index).is_some_and(Option::is_some) {
                continue;
            }
            let Some(scope) = Self::parent_of(file, fact.parent) else {
                continue;
            };
            let Some(owner) = &fact.owner else {
                self.add(file, index, fact, scope.child(&fact.name), scope);
                continue;
            };
            let resolved = {
                let context = self.context(file, scope.clone());
                self.resolve(owner, &context, Wanted::Type)
            };
            if let Some(owner_id) = resolved {
                self.add(file, index, fact, owner_id.child(&fact.name), owner_id);
            } else {
                let id = scope.child(&owner.join("::")).child(&fact.name);
                self.add(file, index, fact, id, scope);
            }
        }
    }

    /// The fields of types, once every method has its place.
    fn fields(&mut self, file: &mut Placed<'_>) {
        let facts = file.facts;
        for (index, fact) in facts.symbols.iter().enumerate() {
            if fact.kind == SymbolKind::Field
                && let Some(parent) = Self::parent_of(file, fact.parent)
            {
                self.add(file, index, fact, parent.child(&fact.name), parent);
            }
        }
    }

    fn kind_of(&self, id: &SymbolId) -> Option<SymbolKind> {
        self.known.get(id).map(|(kind, _)| *kind)
    }

    fn parent_symbol(&self, id: &SymbolId) -> Option<&SymbolId> {
        self.known.get(id).and_then(|(_, parent)| parent.as_ref())
    }

    /// The nearest symbol around `id`, or `id` itself, that passes a test.
    fn enclosing(&self, id: &SymbolId, test: impl Fn(SymbolKind) -> bool) -> Option<SymbolId> {
        let mut current = Some(id.clone());
        while let Some(candidate) = current {
            if self.kind_of(&candidate).is_some_and(&test) {
                return Some(candidate);
            }
            current = self.parent_symbol(&candidate).cloned();
        }
        None
    }

    fn module_of(&self, id: &SymbolId) -> Option<SymbolId> {
        self.enclosing(id, |kind| {
            matches!(kind, SymbolKind::Module | SymbolKind::Package)
        })
    }

    fn context<'a>(&self, file: &'a Placed<'a>, local: SymbolId) -> Context<'a> {
        let module = self
            .module_of(&local)
            .unwrap_or_else(|| file.module.clone());
        Context {
            file,
            local,
            module,
        }
    }

    fn exists(&self, id: &SymbolId, wanted: Wanted) -> bool {
        self.kind_of(id).is_some_and(|kind| wanted.accepts(kind))
    }

    /// Finds what a path names, the way a person reading the file would: in the scopes
    /// around it, then as a path from somewhere, then through what the file imports.
    fn resolve(&self, path: &[String], context: &Context<'_>, wanted: Wanted) -> Option<SymbolId> {
        self.in_scopes(path, context, wanted)
            .or_else(|| self.absolute(path, context, wanted))
            .or_else(|| self.through_imports(path, context, wanted))
    }

    /// Looks in each scope around the code, from the inside out.
    fn in_scopes(
        &self,
        path: &[String],
        context: &Context<'_>,
        wanted: Wanted,
    ) -> Option<SymbolId> {
        let mut current = context.local.clone();
        loop {
            let candidate = join(&current, path);
            if self.exists(&candidate, wanted) {
                return Some(candidate);
            }
            if current == context.module {
                return None;
            }
            current = self.parent_symbol(&current)?.clone();
        }
    }

    /// The imports that are in force where a name is read.
    fn visible_imports<'a>(&self, context: &'a Context<'a>) -> impl Iterator<Item = &'a Import> {
        let local = context.local.clone();
        context
            .file
            .facts
            .imports
            .iter()
            .filter(move |import| match import.scope {
                None => true,
                Some(index) => {
                    context
                        .file
                        .ids
                        .get(index)
                        .cloned()
                        .flatten()
                        .is_some_and(|scope| {
                            self.enclosing(&local, |_| true).is_some()
                                && self.is_within(&local, &scope)
                        })
                }
            })
    }

    fn is_within(&self, id: &SymbolId, ancestor: &SymbolId) -> bool {
        let mut current = Some(id.clone());
        while let Some(candidate) = current {
            if &candidate == ancestor {
                return true;
            }
            current = self.parent_symbol(&candidate).cloned();
        }
        false
    }

    fn through_imports(
        &self,
        path: &[String],
        context: &Context<'_>,
        wanted: Wanted,
    ) -> Option<SymbolId> {
        let (first, rest) = path.split_first()?;
        let named = self
            .visible_imports(context)
            .filter(|import| import.alias.as_ref() == Some(first))
            .find_map(|import| self.absolute(&extended(import, rest), context, wanted));
        named.or_else(|| {
            self.visible_imports(context)
                .filter(|import| import.alias.is_none())
                .find_map(|import| self.absolute(&extended(import, path), context, wanted))
        })
    }

    /// Follows a path that says where it starts from.
    fn absolute(&self, path: &[String], context: &Context<'_>, wanted: Wanted) -> Option<SymbolId> {
        let (first, rest) = path.split_first()?;
        let mut start = match first.as_str() {
            "crate" => context.file.crate_root.clone(),
            "self" => context.module.clone(),
            "super" => self.module_of(self.parent_symbol(&context.module)?)?,
            "Self" => self.enclosing(&context.local, SymbolKind::is_type)?,
            _ => {
                let from_package = SymbolId::new(&path.join("::"));
                let from_module = join(&context.module, path);
                return [from_package, from_module]
                    .into_iter()
                    .find(|id| self.exists(id, wanted));
            }
        };
        let mut rest = rest;
        while let Some((next, others)) = rest.split_first() {
            if next != "super" {
                break;
            }
            start = self.module_of(self.parent_symbol(&start)?)?;
            rest = others;
        }
        let candidate = join(&start, rest);
        self.exists(&candidate, wanted).then_some(candidate)
    }

    /// What a type name means: exactly, or failing that, every type with that name.
    fn types_named(&self, path: &[String], context: &Context<'_>) -> Vec<(SymbolId, Confidence)> {
        if let Some(found) = self.resolve(path, context, Wanted::Type) {
            return vec![(found, Confidence::Exact)];
        }
        let Some(name) = path.last().filter(|name| !is_keyword(name)) else {
            return Vec::new();
        };
        let candidates = self.types_by_name.get(name).cloned().unwrap_or_default();
        let confidence = Confidence::of_candidates(candidates.len());
        candidates.into_iter().map(|id| (id, confidence)).collect()
    }

    fn relate(
        &mut self,
        from: &SymbolId,
        to: SymbolId,
        kind: RelationKind,
        confidence: Confidence,
    ) {
        self.relations.push(Relation {
            from: from.clone(),
            to,
            kind,
            confidence,
        });
    }

    /// The types `impl` blocks extend: their lines count towards the type, and an `impl`
    /// of a trait is an implementation of it.
    fn extensions(&mut self, file: &Placed<'_>) {
        for extension in &file.facts.extensions {
            let Some(scope) = Self::parent_of(file, extension.parent) else {
                continue;
            };
            let context = self.context(file, scope);
            let Some(owner) = self.resolve(&extension.owner, &context, Wanted::Type) else {
                continue;
            };
            self.symbols.add_lines(&owner, extension.code_lines);
            if let Some(interface) = &extension.implements {
                for (target, confidence) in self.types_named(interface, &context) {
                    self.relate(&owner, target, RelationKind::Implements, confidence);
                }
            }
        }
    }

    fn references(&mut self, file: &Placed<'_>) {
        for (index, fact) in file.facts.symbols.iter().enumerate() {
            let Some(id) = file.ids.get(index).cloned().flatten() else {
                continue;
            };
            let context = self.context(file, id.clone());
            for reference in &fact.references {
                let path = &reference.path;
                match reference.kind {
                    ReferenceKind::Type => {
                        let from = if fact.kind == SymbolKind::Field {
                            self.parent_symbol(&id).cloned()
                        } else {
                            Some(id.clone())
                        };
                        let kind = if fact.kind == SymbolKind::Field {
                            RelationKind::Composes
                        } else {
                            RelationKind::Uses
                        };
                        if let Some(from) = from {
                            for (target, confidence) in self.types_named(path, &context) {
                                self.relate(&from, target, kind, confidence);
                            }
                        }
                    }
                    ReferenceKind::Implements | ReferenceKind::Inherits => {
                        let kind = if reference.kind == ReferenceKind::Implements {
                            RelationKind::Implements
                        } else {
                            RelationKind::Inherits
                        };
                        for (target, confidence) in self.types_named(path, &context) {
                            self.relate(&id, target, kind, confidence);
                        }
                    }
                    ReferenceKind::Call => {
                        if let Some(target) = self.resolve(path, &context, Wanted::Callable) {
                            self.relate(&id, target, RelationKind::Calls, Confidence::Exact);
                        }
                    }
                    ReferenceKind::SelfCall => {
                        let owner = self.enclosing(&id, SymbolKind::is_type);
                        let target = owner.map(|owner| join(&owner, path));
                        if let Some(target) =
                            target.filter(|target| self.exists(target, Wanted::Callable))
                        {
                            self.relate(&id, target, RelationKind::Calls, Confidence::Exact);
                        }
                    }
                    ReferenceKind::MethodCall => {
                        let name = path.last().cloned().unwrap_or_default();
                        let candidates =
                            self.methods_by_name.get(&name).cloned().unwrap_or_default();
                        let confidence = Confidence::of_candidates(candidates.len());
                        for target in candidates {
                            self.relate(&id, target, RelationKind::Calls, confidence);
                        }
                    }
                }
            }
        }
    }

    /// A module imports the modules that define what its `use` items name.
    fn imports(&mut self, file: &Placed<'_>) {
        for import in &file.facts.imports {
            let Some(scope) = Self::parent_of(file, import.scope) else {
                continue;
            };
            let context = self.context(file, scope);
            let Some(target) = self.absolute(&import.path, &context, Wanted::Anything) else {
                continue;
            };
            let Some(to) = self.module_of(&target) else {
                continue;
            };
            if to != context.module {
                let from = context.module.clone();
                self.relate(&from, to, RelationKind::Imports, Confidence::Exact);
            }
        }
    }
}

fn extended(import: &Import, rest: &[String]) -> Vec<String> {
    let mut path = import.path.clone();
    path.extend_from_slice(rest);
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract;
    use cyclops_application::ports::{Discovered, FileStamp, SourceError};
    use cyclops_domain::path::RelativePath;
    use cyclops_domain::symbol::Symbol;

    struct NoManifest;

    impl FileSource for NoManifest {
        fn project_name(&self) -> String {
            "p".to_owned()
        }

        fn discover(&self) -> Result<Discovered, SourceError> {
            Ok(Discovered::default())
        }

        fn stamp(&self, path: &RelativePath) -> Result<FileStamp, SourceError> {
            Err(SourceError::Unreadable(path.clone()))
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
        link(&refs, &NoManifest).symbols
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
    fn a_getter_keeps_its_name_and_its_field_is_marked() {
        let symbols = linked(&[(
            "src/lib.rs",
            "pub struct S { x: u8, y: u8 }\nimpl S { fn x(&self) -> u8 { self.x } }",
        )]);
        let kind = |id: &str| {
            symbols
                .iter()
                .find(|symbol| symbol.id.as_str() == id)
                .map(|symbol| symbol.kind)
        };
        assert_eq!(kind("p::S::x"), Some(SymbolKind::Method));
        assert_eq!(kind("p::S::x#field"), Some(SymbolKind::Field));
        assert_eq!(kind("p::S::y"), Some(SymbolKind::Field));
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

    fn relations(files: &[(&str, &str)]) -> Vec<String> {
        let facts: Vec<FileFacts> = files
            .iter()
            .map(|(path, text)| extract(&RelativePath::new(path), text))
            .collect();
        let refs: Vec<&FileFacts> = facts.iter().collect();
        let mut found: Vec<String> = link(&refs, &NoManifest)
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
        found.dedup();
        found
    }

    #[test]
    fn a_static_call_through_a_type_is_exact() {
        let found = relations(&[
            (
                "src/a.rs",
                "pub struct A;\nimpl A { pub fn new() -> A { A } }\n",
            ),
            (
                "src/b.rs",
                "use crate::a::A;\nfn f() { let _ = A::new(); }\n",
            ),
        ]);
        assert!(
            found.contains(&"p::b::f calls p::a::A::new exact".to_owned()),
            "{found:?}"
        );
        assert!(
            found.contains(&"p::b::f uses p::a::A exact".to_owned()),
            "{found:?}"
        );
        assert!(
            found.contains(&"p::b imports p::a exact".to_owned()),
            "{found:?}"
        );
    }

    #[test]
    fn a_supertrait_is_inherited() {
        let found = relations(&[("src/lib.rs", "trait A {}\ntrait B: A {}\n")]);
        assert_eq!(found, ["p::B inherits p::A exact"]);
    }

    #[test]
    fn an_import_from_a_test_module_comes_from_that_module() {
        let found = relations(&[
            ("src/lib.rs", "pub mod a;\n"),
            (
                "src/a.rs",
                "pub fn g() {}\n#[cfg(test)]\nmod tests {\n    use super::g;\n    fn t() { g(); }\n}\n",
            ),
        ]);
        assert!(
            found.contains(&"p::a::tests imports p::a exact".to_owned()),
            "{found:?}"
        );
        assert!(
            found.contains(&"p::a::tests::t calls p::a::g exact".to_owned()),
            "{found:?}"
        );
    }

    #[test]
    fn a_crate_from_outside_is_not_a_relation() {
        let found = relations(&[(
            "src/lib.rs",
            "use std::collections::HashMap;\nfn f(_: HashMap<u8, u8>) {}\n",
        )]);
        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn an_impl_block_adds_its_lines_to_its_type() {
        let symbols = linked(&[(
            "src/lib.rs",
            "struct S;\nimpl S {\n    fn x(&self) {}\n}\nimpl Clone for S {\n    fn clone(&self) -> S { S }\n}\n",
        )]);
        let lines = symbols
            .iter()
            .find(|symbol| symbol.id.as_str() == "p::S")
            .map(|symbol| symbol.code_lines);
        assert_eq!(lines, Some(7));
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
    fn a_module_declared_as_test_code_makes_its_file_test_code() {
        let facts: Vec<FileFacts> = [
            ("src/lib.rs", "#[cfg(test)]\nmod checks;\nfn f() {}\n"),
            ("src/checks.rs", "fn g() {}\n"),
            ("src/checks/deep.rs", "fn h() {}\n"),
            ("src/other.rs", "fn i() {}\n"),
        ]
        .iter()
        .map(|(path, text)| extract(&RelativePath::new(path), text))
        .collect();
        let refs: Vec<&FileFacts> = facts.iter().collect();

        let linked = link(&refs, &NoManifest);

        let role = |id: &str| {
            linked
                .symbols
                .iter()
                .find(|symbol| symbol.id.as_str() == id)
                .map(|symbol| symbol.role)
        };
        assert_eq!(role("p::f"), Some(CodeRole::Production));
        assert_eq!(role("p::other::i"), Some(CodeRole::Production));
        assert_eq!(role("p::checks"), Some(CodeRole::Test));
        assert_eq!(role("p::checks::g"), Some(CodeRole::Test));
        assert_eq!(role("p::checks::deep::h"), Some(CodeRole::Test));
        let mut files: Vec<&str> = linked.test_files.iter().map(RelativePath::as_str).collect();
        files.sort_unstable();
        assert_eq!(files, ["src/checks.rs", "src/checks/deep.rs"]);
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
