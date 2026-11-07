//! Works out which package and module each file belongs to, from Cargo's conventions.

use cyclops_application::ports::FileSource;
use cyclops_domain::path::RelativePath;
use std::collections::{HashMap, HashSet};

/// A Cargo package, or the project itself when there is no manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Package {
    /// The name its code is reached by.
    pub(crate) name: String,
    /// Its folder, relative to the project. Empty for the project root.
    root: String,
    has_manifest: bool,
}

/// Where a file goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    /// It is the root of the package.
    Root,
    /// It is a module, with the path of names from the package down.
    Module(Vec<String>),
    /// It is its own crate, such as an integration test, so its module is its path.
    OwnCrate(String),
}

pub(crate) struct Layout<'a> {
    source: &'a dyn FileSource,
    files: HashSet<String>,
    /// What each folder's manifest says, read once.
    manifests: HashMap<String, Option<String>>,
}

fn without_extension(path: &str) -> &str {
    path.strip_suffix(".rs").unwrap_or(path)
}

fn parent_folder(path: &str) -> Option<&str> {
    if path.is_empty() {
        None
    } else {
        Some(path.rsplit_once('/').map_or("", |(folder, _)| folder))
    }
}

fn joined(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

/// The crate name a manifest gives, if it describes a package.
fn crate_name(manifest: &str) -> Option<String> {
    let table: toml::Table = manifest.parse().ok()?;
    let library = table
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(toml::Value::as_str);
    let package = table
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str);
    Some(library.or(package)?.replace('-', "_"))
}

impl<'a> Layout<'a> {
    pub(crate) fn new(
        paths: impl Iterator<Item = &'a RelativePath>,
        source: &'a dyn FileSource,
    ) -> Self {
        Self {
            source,
            files: paths.map(|path| path.as_str().to_owned()).collect(),
            manifests: HashMap::new(),
        }
    }

    fn manifest_in(&mut self, folder: &str) -> Option<String> {
        if let Some(known) = self.manifests.get(folder) {
            return known.clone();
        }
        let path = RelativePath::new(&joined(folder, "Cargo.toml"));
        let name = self
            .source
            .read(&path)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|text| crate_name(&text));
        self.manifests.insert(folder.to_owned(), name.clone());
        name
    }

    /// The nearest package around a file.
    pub(crate) fn package_of(&mut self, path: &RelativePath) -> Package {
        let mut folder = parent_folder(path.as_str());
        while let Some(current) = folder {
            if let Some(name) = self.manifest_in(current) {
                return Package {
                    name,
                    root: current.to_owned(),
                    has_manifest: true,
                };
            }
            folder = parent_folder(current);
        }
        Package {
            name: self.source.project_name(),
            root: String::new(),
            has_manifest: false,
        }
    }

    /// Where a file goes inside its package.
    pub(crate) fn place_of(&self, path: &RelativePath, package: &Package) -> Place {
        let full = path.as_str();
        let inside = if package.root.is_empty() {
            full
        } else {
            full.strip_prefix(&package.root)
                .map_or(full, |rest| rest.trim_start_matches('/'))
        };
        let source_path = inside.strip_prefix("src/");
        let in_source = source_path.filter(|rest| !rest.starts_with("bin/"));
        let module_path = match (in_source, package.has_manifest) {
            (Some(rest), _) => rest,
            (None, false) => inside,
            (None, true) => return Place::OwnCrate(without_extension(full).to_owned()),
        };
        match module_path {
            "main.rs" if self.has_library(package) => {
                Place::OwnCrate(without_extension(full).to_owned())
            }
            "lib.rs" | "main.rs" => Place::Root,
            other => {
                let mut names: Vec<String> = without_extension(other)
                    .split('/')
                    .map(ToOwned::to_owned)
                    .collect();
                if names.last().is_some_and(|name| name == "mod") {
                    names.pop();
                }
                Place::Module(names)
            }
        }
    }

    fn has_library(&self, package: &Package) -> bool {
        self.files.contains(&joined(&package.root, "src/lib.rs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyclops_application::ports::{Discovered, FileStamp, SourceError};

    struct Project(&'static [(&'static str, &'static str)]);

    impl FileSource for Project {
        fn project_name(&self) -> String {
            "folder".to_owned()
        }

        fn discover(&self) -> Result<Discovered, SourceError> {
            Ok(Discovered {
                files: self
                    .0
                    .iter()
                    .map(|(path, _)| RelativePath::new(path))
                    .collect(),
                unreadable: Vec::new(),
            })
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

    const NAMED: &str = "[package]\nname = \"my-shop\"\n";

    fn place(project: &Project, path: &str) -> (String, Place) {
        let paths = project.discover().expect("listed").files;
        let mut layout = Layout::new(paths.iter(), project);
        let path = RelativePath::new(path);
        let package = layout.package_of(&path);
        let place = layout.place_of(&path, &package);
        (package.name, place)
    }

    fn module(names: &[&str]) -> Place {
        Place::Module(names.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn a_dash_in_a_package_name_is_an_underscore_in_code() {
        let project = Project(&[("Cargo.toml", NAMED), ("src/lib.rs", "")]);
        assert_eq!(
            place(&project, "src/lib.rs"),
            ("my_shop".to_owned(), Place::Root)
        );
    }

    #[test]
    fn the_name_of_a_library_wins() {
        const MANIFEST: &str = "[package]\nname = \"a\"\n[lib]\nname = \"b\"\n";
        let project = Project(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "")]);
        assert_eq!(place(&project, "src/lib.rs").0, "b");
    }

    #[test]
    fn files_map_to_module_paths() {
        let project = Project(&[("Cargo.toml", NAMED), ("src/lib.rs", "")]);
        assert_eq!(place(&project, "src/a.rs").1, module(&["a"]));
        assert_eq!(place(&project, "src/a/mod.rs").1, module(&["a"]));
        assert_eq!(place(&project, "src/a/b.rs").1, module(&["a", "b"]));
    }

    #[test]
    fn a_binary_is_the_root_only_when_there_is_no_library() {
        let alone = Project(&[("Cargo.toml", NAMED), ("src/main.rs", "")]);
        assert_eq!(place(&alone, "src/main.rs").1, Place::Root);

        let both = Project(&[
            ("Cargo.toml", NAMED),
            ("src/lib.rs", ""),
            ("src/main.rs", ""),
        ]);
        assert_eq!(
            place(&both, "src/main.rs").1,
            Place::OwnCrate("src/main".to_owned())
        );
    }

    #[test]
    fn what_cargo_builds_separately_is_its_own_crate() {
        let project = Project(&[("Cargo.toml", NAMED)]);
        for (path, expected) in [
            ("tests/a.rs", "tests/a"),
            ("benches/a.rs", "benches/a"),
            ("examples/a.rs", "examples/a"),
            ("src/bin/a.rs", "src/bin/a"),
            ("build.rs", "build"),
        ] {
            assert_eq!(
                place(&project, path).1,
                Place::OwnCrate(expected.to_owned()),
                "{path}"
            );
        }
    }

    #[test]
    fn each_file_belongs_to_its_nearest_package() {
        let project = Project(&[
            ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
            ("crates/one/Cargo.toml", "[package]\nname = \"one\"\n"),
            ("crates/two/Cargo.toml", "[package]\nname = \"two\"\n"),
        ]);
        assert_eq!(
            place(&project, "crates/two/src/a.rs"),
            ("two".to_owned(), module(&["a"]))
        );
        assert_eq!(
            place(&project, "crates/one/tests/a.rs"),
            (
                "one".to_owned(),
                Place::OwnCrate("crates/one/tests/a".to_owned())
            )
        );
    }

    #[test]
    fn without_a_manifest_the_folder_is_the_package() {
        let project = Project(&[]);
        assert_eq!(
            place(&project, "lib.rs"),
            ("folder".to_owned(), Place::Root)
        );
        assert_eq!(
            place(&project, "a/b.rs"),
            ("folder".to_owned(), module(&["a", "b"]))
        );
        assert_eq!(place(&project, "src/a.rs").1, module(&["a"]));
    }

    #[test]
    fn a_broken_manifest_counts_as_none() {
        let project = Project(&[("Cargo.toml", "not toml ["), ("src/a.rs", "")]);
        assert_eq!(place(&project, "src/a.rs").0, "folder");
    }
}
