//! Places the facts of every TypeScript and JavaScript file in the project. A file is a
//! module, a folder is the module that holds them, and an `index` file is the module of
//! its folder. A module is named after its path from the project root.

use cyclops_application::ports::{FileSource, Linked};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::{SymbolId, SymbolKind};
use cyclops_domain::symbol_set::SymbolSet;
use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::HashMap;

/// A package, or the project itself when there is no `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Package {
    name: String,
    /// Its folder, relative to the project. Empty for the project root.
    root: String,
}

/// Reads the `name` of a `package.json`, which is a valid JavaScript expression.
fn package_name(manifest: &str) -> Option<String> {
    let allocator = Allocator::default();
    let wrapped = format!("({manifest})");
    let parser = Parser::new(&allocator, &wrapped, SourceType::mjs());
    let expression = parser.parse_expression().ok()?;
    let Expression::ObjectExpression(object) = expression.get_inner_expression() else {
        return None;
    };
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        let Expression::StringLiteral(value) = &property.value else {
            return None;
        };
        let name = value.value.as_str()?;
        (property.key.static_name()? == "name").then(|| name.to_owned())
    })
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

/// The path of the module a file is.
fn module_path(file: &str) -> &str {
    let without_extension = [
        ".d.ts", ".d.mts", ".d.cts", ".tsx", ".ts", ".mts", ".cts", ".jsx", ".js", ".mjs", ".cjs",
    ]
    .iter()
    .find_map(|extension| file.strip_suffix(extension))
    .unwrap_or(file);
    match without_extension.rsplit_once('/') {
        Some((folder, "index")) => folder,
        None if without_extension == "index" => "",
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
        let path = if folder.is_empty() {
            "package.json".to_owned()
        } else {
            format!("{folder}/package.json")
        };
        let name = self
            .source
            .read(&RelativePath::new(&path))
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

pub(crate) fn link(facts: &[&FileFacts], source: &dyn FileSource) -> Linked {
    let mut sorted = facts.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut packages = Packages {
        source,
        names: HashMap::new(),
    };
    let mut symbols = SymbolSet::default();
    for file in sorted {
        let package = packages.of(file.path.as_str());
        let module = place(&mut symbols, file, &package);
        add_symbols(&mut symbols, file, &module);
    }
    Linked {
        symbols: symbols.into_symbols(),
        test_files: Vec::new(),
    }
}

/// Makes sure a file's module and the folders around it exist, and says which it is.
fn place(symbols: &mut SymbolSet, file: &FileFacts, package: &Package) -> SymbolId {
    let root = SymbolId::new(&package.name);
    let role = file.role;
    symbols.container(&root, None, &package.name, SymbolKind::Package, role);

    let path = module_path(file.path.as_str());
    if path == package.root {
        return root;
    }
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

fn add_symbols(symbols: &mut SymbolSet, file: &FileFacts, module: &SymbolId) {
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

    fn linked(project: &Project, files: &[(&str, &str)]) -> Vec<Symbol> {
        let facts: Vec<FileFacts> = files
            .iter()
            .map(|(path, text)| extract(&RelativePath::new(path), text))
            .collect();
        let refs: Vec<&FileFacts> = facts.iter().collect();
        link(&refs, project).symbols
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
            package_name("{ \"version\": \"1\", \"name\": \"@a/b\" }").as_deref(),
            Some("@a/b")
        );
        assert_eq!(package_name("{}"), None);
        assert_eq!(package_name("not json ["), None);
    }

    #[test]
    fn modules_are_named_after_their_paths() {
        assert_eq!(module_path("src/a.ts"), "src/a");
        assert_eq!(module_path("src/a.test.ts"), "src/a.test");
        assert_eq!(module_path("src/a.d.ts"), "src/a");
        assert_eq!(module_path("src/a/index.tsx"), "src/a");
        assert_eq!(module_path("index.js"), "");
    }

    #[test]
    fn folders_are_modules_up_to_the_package() {
        let project = Project(&[("package.json", "{\"name\": \"shop\"}")]);
        let symbols = linked(&project, &[("src/a/b.ts", "function f() {}")]);
        assert_eq!(parent_of(&symbols, "src/a/b::f"), Some("src/a/b"));
        assert_eq!(parent_of(&symbols, "src/a/b"), Some("src/a"));
        assert_eq!(parent_of(&symbols, "src/a"), Some("src"));
        assert_eq!(parent_of(&symbols, "src"), Some("shop"));
    }

    #[test]
    fn an_index_at_the_root_is_the_package() {
        let project = Project(&[("package.json", "{\"name\": \"shop\"}")]);
        let symbols = linked(&project, &[("index.ts", "function f() {}")]);
        assert_eq!(parent_of(&symbols, "shop::f"), Some("shop"));
    }

    #[test]
    fn each_file_belongs_to_its_nearest_package() {
        let project = Project(&[
            ("package.json", "{\"name\": \"root\"}"),
            ("packages/a/package.json", "{\"name\": \"a\"}"),
        ]);
        let symbols = linked(&project, &[("packages/a/src/x.ts", "function f() {}")]);
        assert_eq!(parent_of(&symbols, "packages/a/src"), Some("a"));
    }

    #[test]
    fn without_a_manifest_the_folder_is_the_package() {
        let symbols = linked(&Project(&[]), &[("src/a.ts", "")]);
        assert_eq!(parent_of(&symbols, "src"), Some("folder"));
    }

    #[test]
    fn two_files_of_one_module_stay_apart() {
        let symbols = linked(
            &Project(&[]),
            &[
                ("src/a.ts", "function f() {}"),
                ("src/a.js", "function f() {}"),
            ],
        );
        assert!(parent_of(&symbols, "src/a::f").is_some());
        assert!(parent_of(&symbols, "src/a::f#2").is_some());
    }
}
