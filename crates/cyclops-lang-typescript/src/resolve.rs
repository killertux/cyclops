//! Works out what names mean across files: which file a module specifier is, where a
//! barrel file's names really come from, and the relations between symbols.

use cyclops_application::ports::FileSource;
use cyclops_domain::facts::{FileFacts, Import, ReferenceKind};
use cyclops_domain::path::RelativePath;
use cyclops_domain::relation::{Confidence, Relation, RelationKind};
use cyclops_domain::symbol::{SymbolId, SymbolKind};
use cyclops_domain::symbol_set::SymbolSet;
use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind};
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::cell::RefCell;
use std::collections::HashMap;

/// A file, with the module it is and the ids its symbols got.
pub(crate) struct Placed<'a> {
    pub(crate) facts: &'a FileFacts,
    pub(crate) module: SymbolId,
    pub(crate) ids: Vec<SymbolId>,
}

/// How far to follow re-exports before giving up, so that a loop of them ends.
const MAX_REEXPORTS: usize = 8;

/// The extensions an import may leave out, in the order they are tried.
const EXTENSIONS: [&str; 9] = [
    ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
];

/// What a `tsconfig.json` says about finding modules.
#[derive(Debug, Clone)]
struct TsConfig {
    /// The folder `baseUrl` points to, relative to the project.
    base: String,
    /// Each pattern of `paths`, with where it leads.
    paths: Vec<(String, Vec<String>)>,
}

fn strings(object: &ObjectExpression<'_>, key: &str) -> Option<Vec<String>> {
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        if property.key.static_name()? != key {
            return None;
        }
        match &property.value {
            Expression::StringLiteral(text) => Some(vec![text.value.as_str()?.to_owned()]),
            Expression::ArrayExpression(items) => Some(
                items
                    .elements
                    .iter()
                    .filter_map(|item| match item.as_expression()? {
                        Expression::StringLiteral(text) => Some(text.value.as_str()?.to_owned()),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        }
    })
}

fn object<'n, 'a>(object: &'n ObjectExpression<'a>, key: &str) -> Option<&'n ObjectExpression<'a>> {
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        if property.key.static_name()? != key {
            return None;
        }
        match &property.value {
            Expression::ObjectExpression(inner) => Some(&**inner),
            _ => None,
        }
    })
}

/// Reads a `tsconfig.json`, which allows comments and trailing commas, as JavaScript does.
fn read_tsconfig(folder: &str, text: &str) -> Option<TsConfig> {
    let allocator = Allocator::default();
    let wrapped = format!("({text})");
    let expression = Parser::new(&allocator, &wrapped, SourceType::mjs())
        .parse_expression()
        .ok()?;
    let Expression::ObjectExpression(root) = expression.get_inner_expression() else {
        return None;
    };
    let options = object(root, "compilerOptions");
    let base_url = options
        .and_then(|options| strings(options, "baseUrl"))
        .and_then(|values| values.into_iter().next())
        .unwrap_or_else(|| ".".to_owned());
    let paths = options
        .and_then(|options| object(options, "paths"))
        .map(|paths| {
            paths
                .properties
                .iter()
                .filter_map(|property| {
                    let ObjectPropertyKind::ObjectProperty(property) = property else {
                        return None;
                    };
                    let pattern = property.key.static_name()?.as_str()?.to_owned();
                    let Expression::ArrayExpression(targets) = &property.value else {
                        return None;
                    };
                    let targets = targets
                        .elements
                        .iter()
                        .filter_map(|item| match item.as_expression()? {
                            Expression::StringLiteral(text) => {
                                Some(text.value.as_str()?.to_owned())
                            }
                            _ => None,
                        })
                        .collect();
                    Some((pattern, targets))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(TsConfig {
        base: normalize(&join(folder, &base_url)),
        paths,
    })
}

fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

fn join(folder: &str, path: &str) -> String {
    if folder.is_empty() {
        path.to_owned()
    } else {
        format!("{folder}/{path}")
    }
}

/// Takes out `.` and `..`. A path that climbs out of the project is not ours.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Matches a `paths` pattern with at most one `*`, and says what the `*` stood for.
fn wildcard<'s>(pattern: &str, specifier: &'s str) -> Option<&'s str> {
    match pattern.split_once('*') {
        Some((before, after)) => specifier
            .strip_prefix(before)
            .and_then(|rest| rest.strip_suffix(after)),
        None => (pattern == specifier).then_some(""),
    }
}

pub(crate) struct Resolver<'a> {
    files: &'a [Placed<'a>],
    by_path: HashMap<&'a str, usize>,
    symbols: &'a SymbolSet,
    source: &'a dyn FileSource,
    tsconfigs: RefCell<HashMap<String, Option<TsConfig>>>,
    types_by_name: HashMap<&'a str, Vec<SymbolId>>,
    methods_by_name: HashMap<&'a str, Vec<SymbolId>>,
}

impl<'a> Resolver<'a> {
    pub(crate) fn new(
        files: &'a [Placed<'a>],
        symbols: &'a SymbolSet,
        source: &'a dyn FileSource,
    ) -> Self {
        let by_path = files
            .iter()
            .enumerate()
            .map(|(index, file)| (file.facts.path.as_str(), index))
            .collect();
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
            by_path,
            symbols,
            source,
            tsconfigs: RefCell::new(HashMap::new()),
            types_by_name,
            methods_by_name,
        }
    }

    /// The nearest `tsconfig.json` around a folder.
    fn tsconfig(&self, folder: &str) -> Option<TsConfig> {
        let mut current = Some(folder.to_owned());
        while let Some(candidate) = current {
            let known = self.tsconfigs.borrow().get(&candidate).cloned();
            let config = known.unwrap_or_else(|| {
                let path = RelativePath::new(&join(&candidate, "tsconfig.json"));
                let read = self
                    .source
                    .read(&path)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .and_then(|text| read_tsconfig(&candidate, &text));
                self.tsconfigs
                    .borrow_mut()
                    .insert(candidate.clone(), read.clone());
                read
            });
            if config.is_some() {
                return config;
            }
            current = (!candidate.is_empty()).then(|| folder_of(&candidate).to_owned());
        }
        None
    }

    /// The file a path stands for, trying what an import may leave out.
    fn probe(&self, base: &str) -> Option<usize> {
        if let Some(index) = self.by_path.get(base) {
            return Some(*index);
        }
        let stem = [".js", ".jsx", ".mjs", ".cjs"]
            .iter()
            .find_map(|extension| base.strip_suffix(extension));
        let mut candidates = Vec::new();
        for extension in EXTENSIONS {
            candidates.push(format!("{base}{extension}"));
        }
        for extension in EXTENSIONS {
            candidates.push(format!("{base}/index{extension}"));
        }
        if let Some(stem) = stem {
            for extension in [".ts", ".tsx", ".mts", ".cts"] {
                candidates.push(format!("{stem}{extension}"));
            }
        }
        candidates
            .iter()
            .find_map(|candidate| self.by_path.get(candidate.as_str()).copied())
    }

    /// The file a module specifier names, if it is one of the project's.
    fn file_for(&self, from: usize, specifier: &str) -> Option<usize> {
        let path = self.files.get(from)?.facts.path.as_str();
        let folder = folder_of(path);
        if specifier.starts_with("./")
            || specifier.starts_with("../")
            || specifier == "."
            || specifier == ".."
        {
            return self.probe(&normalize(&join(folder, specifier)));
        }
        let config = self.tsconfig(folder)?;
        for (pattern, targets) in &config.paths {
            if let Some(star) = wildcard(pattern, specifier) {
                let found = targets.iter().find_map(|target| {
                    self.probe(&normalize(&join(&config.base, &target.replace('*', star))))
                });
                if found.is_some() {
                    return found;
                }
            }
        }
        self.probe(&normalize(&join(&config.base, specifier)))
    }

    /// Where a name a module offers is really defined: in the module itself, or behind
    /// its re-exports.
    fn lookup(&self, file: usize, name: &str, depth: usize) -> Option<(usize, Option<SymbolId>)> {
        if depth > MAX_REEXPORTS {
            return None;
        }
        let placed = self.files.get(file)?;
        let local = placed
            .facts
            .symbols
            .iter()
            .zip(&placed.ids)
            .find(|(fact, _)| fact.parent.is_none() && fact.name == name);
        if let Some((_, id)) = local {
            return Some((file, Some(id.clone())));
        }
        for export in &placed.facts.exports {
            let Some(target) = self.file_for(file, &export.from) else {
                continue;
            };
            match &export.name {
                Some(exported) if exported == name => {
                    let original = export.original.as_deref().unwrap_or(name);
                    return self
                        .lookup(target, original, depth + 1)
                        .or(Some((target, None)));
                }
                None => {
                    if let Some(found) = self.lookup(target, name, depth + 1) {
                        return Some(found);
                    }
                }
                Some(_) => {}
            }
        }
        None
    }

    /// The module an import really depends on, past any barrel file.
    fn imported_module(&self, file: usize, import: &Import) -> Option<usize> {
        let target = self.file_for(file, import.from.as_deref()?)?;
        let defining = match import.path.first() {
            Some(name) if name != "default" => self.lookup(target, name, 0).map(|(found, _)| found),
            _ => None,
        };
        Some(defining.unwrap_or(target))
    }

    fn kind_of(&self, id: &SymbolId) -> Option<SymbolKind> {
        self.symbols.get(id).map(|symbol| symbol.kind)
    }

    fn join(start: &SymbolId, path: &[String]) -> SymbolId {
        path.iter().fold(start.clone(), |id, name| id.child(name))
    }

    /// What a name written in a file means, exactly: something the file defines, or
    /// something it imports.
    fn exact(
        &self,
        file: usize,
        path: &[String],
        wanted: impl Fn(SymbolKind) -> bool,
    ) -> Option<SymbolId> {
        let placed = self.files.get(file)?;
        let (first, rest) = path.split_first()?;
        let local = placed
            .facts
            .symbols
            .iter()
            .zip(&placed.ids)
            .find(|(fact, _)| fact.parent.is_none() && &fact.name == first)
            .map(|(_, id)| Self::join(id, rest));
        let imported = || {
            placed.facts.imports.iter().find_map(|import| {
                if import.alias.as_ref() != Some(first) {
                    return None;
                }
                let target = self.file_for(file, import.from.as_deref()?)?;
                let (name, rest) = match import.path.first() {
                    Some(name) => (name.as_str(), rest),
                    None => rest
                        .split_first()
                        .map(|(name, rest)| (name.as_str(), rest))?,
                };
                let (_, id) = self.lookup(target, name, 0)?;
                Some(Self::join(&id?, rest))
            })
        };
        local
            .filter(|id| self.kind_of(id).is_some_and(&wanted))
            .or_else(|| imported().filter(|id| self.kind_of(id).is_some_and(&wanted)))
    }

    /// What a type name means: exactly, or failing that, every type with that name.
    fn types_named(&self, file: usize, path: &[String]) -> Vec<(SymbolId, Confidence)> {
        if let Some(found) = self.exact(file, path, SymbolKind::is_type) {
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

    pub(crate) fn relations(&self) -> Vec<Relation> {
        let mut relations = Vec::new();
        let mut relate =
            |from: &SymbolId, to: SymbolId, kind: RelationKind, confidence: Confidence| {
                relations.push(Relation {
                    from: from.clone(),
                    to,
                    kind,
                    confidence,
                });
            };
        for (index, file) in self.files.iter().enumerate() {
            for import in &file.facts.imports {
                if let Some(target) = self.imported_module(index, import)
                    && let Some(to) = self.files.get(target).map(|target| target.module.clone())
                    && to != file.module
                {
                    relate(&file.module, to, RelationKind::Imports, Confidence::Exact);
                }
            }
            for (fact, id) in file.facts.symbols.iter().zip(&file.ids) {
                for reference in &fact.references {
                    let path = &reference.path;
                    match reference.kind {
                        ReferenceKind::Type => {
                            let (from, kind) = if fact.kind == SymbolKind::Field {
                                (
                                    self.symbols
                                        .get(id)
                                        .and_then(|symbol| symbol.parent.clone()),
                                    RelationKind::Composes,
                                )
                            } else {
                                (Some(id.clone()), RelationKind::Uses)
                            };
                            if let Some(from) = from {
                                for (target, confidence) in self.types_named(index, path) {
                                    relate(&from, target, kind, confidence);
                                }
                            }
                        }
                        ReferenceKind::Implements | ReferenceKind::Inherits => {
                            let kind = if reference.kind == ReferenceKind::Implements {
                                RelationKind::Implements
                            } else {
                                RelationKind::Inherits
                            };
                            for (target, confidence) in self.types_named(index, path) {
                                relate(id, target, kind, confidence);
                            }
                        }
                        ReferenceKind::Call => {
                            let callable =
                                |kind| matches!(kind, SymbolKind::Function | SymbolKind::Method);
                            if let Some(target) = self.exact(index, path, callable) {
                                relate(id, target, RelationKind::Calls, Confidence::Exact);
                            }
                        }
                        ReferenceKind::SelfCall => {
                            let target = self.owner_type(id).map(|owner| Self::join(&owner, path));
                            if let Some(target) = target
                                .filter(|target| self.kind_of(target) == Some(SymbolKind::Method))
                            {
                                relate(id, target, RelationKind::Calls, Confidence::Exact);
                            }
                        }
                        ReferenceKind::MethodCall => {
                            let name = path.last().map(String::as_str).unwrap_or_default();
                            let candidates =
                                self.methods_by_name.get(name).cloned().unwrap_or_default();
                            let confidence = Confidence::of_candidates(candidates.len());
                            for target in candidates {
                                relate(id, target, RelationKind::Calls, confidence);
                            }
                        }
                    }
                }
            }
        }
        relations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_tidied() {
        assert_eq!(normalize("src/a/../b/./c"), "src/b/c");
        assert_eq!(normalize("./a"), "a");
    }

    #[test]
    fn a_pattern_says_what_its_star_stood_for() {
        assert_eq!(wildcard("@lib/*", "@lib/helper"), Some("helper"));
        assert_eq!(wildcard("@lib/*", "react"), None);
        assert_eq!(wildcard("exact", "exact"), Some(""));
    }

    #[test]
    fn a_tsconfig_may_have_comments_and_trailing_commas() {
        let config = read_tsconfig(
            "web",
            "{\n  // options\n  \"compilerOptions\": { \"baseUrl\": \"src\", \"paths\": { \"@/*\": [\"*\"], }, },\n}",
        )
        .expect("read");
        assert_eq!(config.base, "web/src");
        assert_eq!(config.paths, [("@/*".to_owned(), vec!["*".to_owned()])]);
    }
}
