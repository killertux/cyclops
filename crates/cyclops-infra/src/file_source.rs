//! Finds and reads the files of a project on disk.

use cyclops_application::ports::{Discovered, FileSource, SourceError};
use cyclops_domain::path::RelativePath;
use ignore::WalkBuilder;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Folders that hold what other tools installed or built, whatever `.gitignore` says.
const NEVER_ANALYZED: [&str; 2] = ["node_modules", "target"];

/// The folder given for a project does not exist.
#[derive(Debug, Error)]
#[error("Project not found: {}", .0.display())]
pub struct ProjectNotFound(PathBuf);

/// A project folder.
#[derive(Debug, Clone)]
pub struct ProjectFiles {
    root: PathBuf,
}

impl ProjectFiles {
    /// Opens a project.
    ///
    /// # Errors
    ///
    /// Fails if the folder does not exist.
    pub fn open(root: &Path) -> Result<Self, ProjectNotFound> {
        if root.is_dir() {
            Ok(Self {
                root: root.to_path_buf(),
            })
        } else {
            Err(ProjectNotFound(root.to_path_buf()))
        }
    }

    fn relative(&self, path: &Path) -> Option<RelativePath> {
        let inside = path.strip_prefix(&self.root).ok()?;
        Some(RelativePath::new(&inside.to_string_lossy()))
    }
}

fn is_never_analyzed(name: &OsStr) -> bool {
    NEVER_ANALYZED.iter().any(|folder| name == *folder)
}

/// What a walk could not get into.
fn failed_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            failed_path(err)
        }
        _ => None,
    }
}

impl FileSource for ProjectFiles {
    fn project_name(&self) -> String {
        let absolute = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        absolute.file_name().map_or_else(
            || "project".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    fn discover(&self) -> Result<Discovered, SourceError> {
        let walker = WalkBuilder::new(&self.root)
            .hidden(true)
            .parents(false)
            .git_global(false)
            .require_git(false)
            .filter_entry(|entry| {
                let is_folder = entry.file_type().is_some_and(|kind| kind.is_dir());
                !(is_folder && is_never_analyzed(entry.file_name()))
            })
            .build();
        let mut discovered = Discovered::default();
        for entry in walker {
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_some_and(|kind| kind.is_file())
                        && let Some(path) = self.relative(entry.path())
                    {
                        discovered.files.push(path);
                    }
                }
                Err(error) => {
                    let path = failed_path(&error).and_then(|path| self.relative(path));
                    let path = path.ok_or_else(|| SourceError::Unlistable(error.to_string()))?;
                    discovered.unreadable.push(path);
                }
            }
        }
        discovered.files.sort();
        discovered.unreadable.sort();
        Ok(discovered)
    }

    fn read(&self, path: &RelativePath) -> Result<Vec<u8>, SourceError> {
        fs::read(self.root.join(path.as_str())).map_err(|_| SourceError::Unreadable(path.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::os::unix::fs::PermissionsExt;
    use std::process;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A folder that is gone at the end of a test.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(files: &[&str]) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let root = env::temp_dir().join(format!("cyclops-infra-{}-{id}", process::id()));
            for file in files {
                let path = root.join(file);
                fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
                fs::write(path, "").expect("a file");
            }
            Self(root)
        }

        fn discover(&self) -> Discovered {
            ProjectFiles::open(&self.0)
                .expect("the project exists")
                .discover()
                .expect("it can be listed")
        }

        fn files(&self) -> Vec<String> {
            let files = self.discover().files;
            files.iter().map(ToString::to_string).collect()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("the folder is removed");
        }
    }

    #[test]
    fn a_folder_that_does_not_exist_is_not_a_project() {
        let error = ProjectFiles::open(Path::new("/no/such/folder")).expect_err("it fails");
        assert_eq!(error.to_string(), "Project not found: /no/such/folder");
    }

    #[test]
    fn files_come_in_order_of_path() {
        let project = Scratch::new(&["b.rs", "a/c.rs", "a.rs"]);
        assert_eq!(project.files(), ["a.rs", "a/c.rs", "b.rs"]);
    }

    #[test]
    fn what_tools_put_in_a_project_is_left_alone() {
        let project = Scratch::new(&[
            "src/a.rs",
            ".hidden/b.rs",
            ".cyclops/cache/c.rs",
            "node_modules/pkg/d.js",
            "sub/node_modules/e.js",
            "target/debug/f.rs",
        ]);
        assert_eq!(project.files(), ["src/a.rs"]);
    }

    #[test]
    fn a_file_named_like_a_folder_we_skip_is_kept() {
        let project = Scratch::new(&["target"]);
        assert_eq!(project.files(), ["target"]);
    }

    #[test]
    fn gitignore_counts_without_a_repository() {
        let project = Scratch::new(&["a.rs", "b.rs"]);
        fs::write(project.0.join(".gitignore"), "b.rs\n").expect("a file");
        assert_eq!(project.files(), ["a.rs"]);
    }

    #[test]
    fn a_folder_that_cannot_be_listed_does_not_stop_the_rest() {
        let project = Scratch::new(&["a.rs", "locked/b.rs"]);
        let locked = project.0.join("locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("locked");

        let discovered = project.discover();

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlocked");
        assert_eq!(discovered.files, [RelativePath::new("a.rs")]);
        assert_eq!(discovered.unreadable, [RelativePath::new("locked")]);
    }

    #[test]
    fn the_project_is_named_after_its_folder_even_from_a_relative_path() {
        let project = Scratch::new(&[]);
        fs::create_dir_all(project.0.join("my-project")).expect("a folder");
        let files = ProjectFiles::open(&project.0.join("my-project/.")).expect("it exists");
        assert_eq!(files.project_name(), "my-project");
    }
}
