//! Learning that files of a project changed, so that an interface can say its results
//! are stale.

use ignore::gitignore::Gitignore;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Component, Path, PathBuf};

/// Folders that never hold the project's own code, as the analysis sees it.
const NEVER_ANALYZED: [&str; 2] = ["node_modules", "target"];

/// Watches a project for as long as it is kept.
pub struct ProjectWatcher {
    _watcher: RecommendedWatcher,
}

/// Whether a change to this path can change an analysis: it is inside the project, not
/// hidden, not built or installed by a tool, and not ignored.
fn counts(root: &Path, ignored: &Gitignore, path: &Path) -> bool {
    let Ok(inside) = path.strip_prefix(root) else {
        return false;
    };
    let skipped = inside.components().any(|component| match component {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            name.starts_with('.') || NEVER_ANALYZED.contains(&name.as_ref())
        }
        _ => false,
    });
    !skipped
        && !ignored
            .matched_path_or_any_parents(inside, path.is_dir())
            .is_ignore()
}

impl ProjectWatcher {
    /// Starts watching, and calls `changed` for each change that counts.
    ///
    /// # Errors
    ///
    /// Says why the project cannot be watched.
    /// `changed` says whether anyone still wants to know.
    pub fn start(root: &Path, changed: Box<dyn Fn() -> bool + Send>) -> Result<Self, String> {
        let root: PathBuf = root.canonicalize().map_err(|error| error.to_string())?;
        let (ignored, _) = Gitignore::new(root.join(".gitignore"));
        let project = root.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else {
                return;
            };
            let relevant = matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            );
            if relevant
                && event
                    .paths
                    .iter()
                    .any(|path| counts(&project, &ignored, path))
            {
                changed();
            }
        })
        .map_err(|error| error.to_string())?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| error.to_string())?;
        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_built_and_ignored_files_do_not_count() {
        let root = std::env::temp_dir().join(format!("cyclops-watch-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("a folder");
        std::fs::write(root.join(".gitignore"), "generated.rs\n").expect("written");
        let (ignored, _) = Gitignore::new(root.join(".gitignore"));
        assert!(counts(&root, &ignored, &root.join("src/a.rs")));
        assert!(!counts(
            &root,
            &ignored,
            &root.join(".cyclops/cache/cyclops.db")
        ));
        assert!(!counts(&root, &ignored, &root.join("target/debug/a")));
        assert!(!counts(&root, &ignored, &root.join("generated.rs")));
        assert!(!counts(&root, &ignored, Path::new("/elsewhere/a.rs")));
        std::fs::remove_dir_all(&root).expect("removed");
    }
}
