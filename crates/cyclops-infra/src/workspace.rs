//! Whether a Cargo workspace would take a folder as one of its crates. Cargo takes every
//! folder a `members` pattern matches, hidden ones too, and stops building when one has
//! no `Cargo.toml`.

use std::fs;
use std::path::{Component, Path};
use thiserror::Error;

/// A Cargo workspace that would take `.cyclops/` as a member, and how to go on.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "Cyclops cannot keep .cyclops/ here: the Cargo workspace in {manifest} would take it as \
     a member through \"{pattern}\", and stop building. Run Cyclops at the workspace root, \
     or add \"{excluded}\" to the workspace's exclude."
)]
pub struct TakenByWorkspace {
    /// The workspace's `Cargo.toml`, from the project.
    manifest: String,
    /// The `members` pattern that matches.
    pattern: String,
    /// What the workspace's `exclude` would need.
    excluded: String,
}

/// The first workspace, from `project` upward, that would take `project/folder` as a
/// member. `project` must be a real path, with no `..` in it.
pub fn taking(project: &Path, folder: &str) -> Option<TakenByWorkspace> {
    project.ancestors().enumerate().find_map(|(up, root)| {
        let inside = names(project.strip_prefix(root).ok()?)?;
        let manifest = format!("{}Cargo.toml", "../".repeat(up));
        workspace_at(root)?.taking(&inside, folder, manifest)
    })
}

/// The patterns of a workspace.
struct Workspace {
    members: Vec<String>,
    exclude: Vec<String>,
}

/// The workspace whose `Cargo.toml` is in a folder. A manifest that cannot be read, or
/// is not valid TOML, is no workspace Cargo can build either, so `.cyclops/` cannot
/// break it.
fn workspace_at(root: &Path) -> Option<Workspace> {
    let text = fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let manifest: toml::Table = text.parse().ok()?;
    let workspace = manifest.get("workspace")?.as_table()?;
    let strings = |key: &str| -> Vec<String> {
        workspace
            .get(key)
            .and_then(toml::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(Workspace {
        members: strings("members"),
        exclude: strings("exclude"),
    })
}

impl Workspace {
    /// Whether the workspace takes `folder`, in the project at `inside` from its root.
    fn taking(&self, inside: &[&str], folder: &str, manifest: String) -> Option<TakenByWorkspace> {
        let mut path = inside.to_vec();
        path.push(folder);
        if self
            .exclude
            .iter()
            .any(|excluded| path.starts_with(&parts(excluded)))
        {
            return None;
        }
        let pattern = self
            .members
            .iter()
            .find(|pattern| path_matches(&parts(pattern), &path))?;
        Some(TakenByWorkspace {
            manifest,
            pattern: pattern.clone(),
            excluded: path.join("/"),
        })
    }
}

/// The names of a relative path, or nothing if one is not plain UTF-8.
fn names(path: &Path) -> Option<Vec<&str>> {
    path.components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect()
}

/// The names of a path as a manifest writes it.
fn parts(written: &str) -> Vec<&str> {
    written
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect()
}

/// Whether a pattern matches a path, name by name, as Cargo's globs do. `**` stands for
/// any number of folders.
fn path_matches(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => {
            path_matches(rest, path)
                || path
                    .split_first()
                    .is_some_and(|(_, tail)| path_matches(pattern, tail))
        }
        Some((first, rest)) => path
            .split_first()
            .is_some_and(|(name, tail)| name_matches(first, name) && path_matches(rest, tail)),
    }
}

/// Whether a pattern matches a name. `*` and `?` match a leading dot as well.
fn name_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    chars_match(&pattern, &name)
}

fn chars_match(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => {
            chars_match(rest, name)
                || name
                    .split_first()
                    .is_some_and(|(_, tail)| chars_match(pattern, tail))
        }
        Some(('?', rest)) => name
            .split_first()
            .is_some_and(|(_, tail)| chars_match(rest, tail)),
        Some(('[', rest)) if let Some((class, after)) = Class::read(rest) => name
            .split_first()
            .is_some_and(|(found, tail)| class.has(*found) && chars_match(after, tail)),
        Some((expected, rest)) => name
            .split_first()
            .is_some_and(|(found, tail)| found == expected && chars_match(rest, tail)),
    }
}

/// A `[...]` of a pattern: characters and ranges, or all but them after `!`.
struct Class {
    negated: bool,
    ranges: Vec<(char, char)>,
}

impl Class {
    /// Reads a class after its `[`, and returns what follows its `]`. Nothing if it is
    /// never closed, and then the `[` is an ordinary character.
    fn read(after: &[char]) -> Option<(Self, &[char])> {
        let (negated, mut rest) = match after.split_first() {
            Some(('!', rest)) => (true, rest),
            _ => (false, after),
        };
        let mut ranges = Vec::new();
        let mut first = true;
        loop {
            match rest {
                [']', tail @ ..] if !first => {
                    return Some((Self { negated, ranges }, tail));
                }
                [low, '-', high, tail @ ..] if *high != ']' => {
                    ranges.push((*low, *high));
                    rest = tail;
                }
                [single, tail @ ..] => {
                    ranges.push((*single, *single));
                    rest = tail;
                }
                [] => return None,
            }
            first = false;
        }
    }

    fn has(&self, found: char) -> bool {
        let listed = self
            .ranges
            .iter()
            .any(|(low, high)| (*low..=*high).contains(&found));
        listed != self.negated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_matches_a_hidden_folder_as_cargo_does() {
        assert!(name_matches("*", ".cyclops"));
        assert!(name_matches("?cyclops", ".cyclops"));
        assert!(name_matches("c*s", "crates"));
        assert!(!name_matches("c*s", "crate"));
    }

    #[test]
    fn classes_list_ranges_and_may_be_negated() {
        assert!(!name_matches("[!.]*", ".cyclops"));
        assert!(name_matches("[!.]*", "engine"));
        assert!(name_matches("c[0-9][0-9]", "c07"));
        assert!(!name_matches("c[0-9]", "cx"));
        assert!(name_matches("[]]", "]"));
        assert!(name_matches("[x", "[x"));
    }

    #[test]
    fn patterns_match_folder_by_folder() {
        assert!(path_matches(&parts("crates/*"), &["crates", ".cyclops"]));
        assert!(!path_matches(
            &parts("crates/*"),
            &["crates", "a", ".cyclops"]
        ));
        assert!(path_matches(&parts("./crates/*/"), &["crates", ".cyclops"]));
        assert!(path_matches(&parts("**"), &["crates", "a", ".cyclops"]));
        assert!(path_matches(&parts("crates/**/*"), &["crates", ".cyclops"]));
        assert!(!path_matches(&parts("crates"), &["crates", ".cyclops"]));
    }

    fn workspace(members: &[&str], exclude: &[&str]) -> Workspace {
        let owned = |list: &[&str]| list.iter().map(|item| (*item).to_owned()).collect();
        Workspace {
            members: owned(members),
            exclude: owned(exclude),
        }
    }

    #[test]
    fn what_the_workspace_excludes_is_not_taken() {
        let taking = |exclude: &[&str]| {
            workspace(&["crates/*"], exclude)
                .taking(&["crates"], ".cyclops", "../Cargo.toml".to_owned())
                .is_some()
        };
        assert!(taking(&[]));
        assert!(!taking(&["crates/.cyclops"]));
        assert!(!taking(&["crates"]));
        assert!(taking(&["crates/engine"]));
    }

    #[test]
    fn the_message_names_the_manifest_the_pattern_and_the_exclude() {
        let taken = workspace(&["crates/*"], &[])
            .taking(&["crates"], ".cyclops", "../Cargo.toml".to_owned())
            .unwrap();
        let message = taken.to_string();
        assert!(message.contains("the Cargo workspace in ../Cargo.toml"));
        assert!(message.contains("through \"crates/*\""));
        assert!(message.contains("add \"crates/.cyclops\" to the workspace's exclude"));
    }

    #[test]
    fn workspaces_are_found_upward_from_the_project() {
        let root = std::env::temp_dir().join(format!("cyclops-workspace-{}", std::process::id()));
        let crates = root.join("crates");
        fs::create_dir_all(crates.join("engine")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        let root = root.canonicalize().unwrap();
        let crates = crates.canonicalize().unwrap();
        let taken = taking(&crates, ".cyclops").unwrap();
        assert_eq!(taken.manifest, "../Cargo.toml");
        assert_eq!(taken.excluded, "crates/.cyclops");
        assert_eq!(taking(&crates.join("engine"), ".cyclops"), None);
        assert_eq!(taking(&root, ".cyclops"), None);
        fs::remove_dir_all(&root).unwrap();
    }
}
