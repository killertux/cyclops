//! Reads a project's git history, with the files each commit changed.

use cyclops_application::ports::{Commit, FileChange, HistoryError, HistorySource, NewHistory};
use gix::object::tree::diff::ChangeDetached;
use std::path::Path;

/// The history of the repository a project is in. The project may be a folder inside
/// the repository, and then only its files count, with paths relative to it.
pub struct GitHistory {
    repository: gix::Repository,
    /// Where the project is inside the repository, ending with `/`, or empty.
    prefix: String,
}

fn unreadable(error: impl std::fmt::Display) -> HistoryError {
    HistoryError::Unreadable(error.to_string())
}

impl GitHistory {
    /// The repository around a project, if there is one.
    #[must_use]
    pub fn open(project: &Path) -> Option<Self> {
        let repository = gix::discover(project).ok()?;
        let workdir = repository.workdir()?.canonicalize().ok()?;
        let project = project.canonicalize().ok()?;
        let inside = project.strip_prefix(&workdir).ok()?;
        let prefix = inside.to_string_lossy().replace('\\', "/");
        let prefix = if prefix.is_empty() {
            prefix
        } else {
            format!("{prefix}/")
        };
        Some(Self { repository, prefix })
    }

    /// A path in the repository as the project sees it, if it is in the project.
    fn inside(&self, path: &str) -> Option<String> {
        path.strip_prefix(&self.prefix)
            .map(ToOwned::to_owned)
            .filter(|path| !path.is_empty())
    }

    fn changes(
        &self,
        info: &gix::revision::walk::Info<'_>,
    ) -> Result<Vec<FileChange>, HistoryError> {
        let commit = info.object().map_err(unreadable)?;
        let tree = commit.tree().map_err(unreadable)?;
        let parent = match info.parent_ids().next() {
            Some(id) => id
                .object()
                .map_err(unreadable)?
                .into_commit()
                .tree()
                .map_err(unreadable)?,
            None => self.repository.empty_tree(),
        };
        let mut options = gix::diff::Options::default();
        options.track_path();
        options.track_rewrites(Some(gix::diff::Rewrites::default()));
        let changes = self
            .repository
            .diff_tree_to_tree(Some(&parent), Some(&tree), Some(options))
            .map_err(unreadable)?;
        let mut files = Vec::new();
        for change in changes {
            let (location, previous, mode) = match &change {
                ChangeDetached::Addition {
                    location,
                    entry_mode,
                    ..
                }
                | ChangeDetached::Deletion {
                    location,
                    entry_mode,
                    ..
                }
                | ChangeDetached::Modification {
                    location,
                    entry_mode,
                    ..
                } => (location, None, entry_mode),
                ChangeDetached::Rewrite {
                    source_location,
                    location,
                    entry_mode,
                    ..
                } => (location, Some(source_location), entry_mode),
            };
            if mode.is_tree() {
                continue;
            }
            let Some(path) = self.inside(&location.to_string()) else {
                continue;
            };
            let previous = previous.and_then(|previous| self.inside(&previous.to_string()));
            files.push(FileChange { path, previous });
        }
        Ok(files)
    }
}

impl HistorySource for GitHistory {
    fn read_since(&self, known: Option<&str>) -> Result<NewHistory, HistoryError> {
        let head = self
            .repository
            .head_commit()
            .map_err(|_| HistoryError::NoHistory)?;
        let head_id = head.id.to_string();
        let walk = self
            .repository
            .rev_walk([head.id])
            .all()
            .map_err(unreadable)?;
        let mut commits = Vec::new();
        let mut reached_known = false;
        for info in walk {
            let info = info.map_err(unreadable)?;
            let id = info.id.to_string();
            if known == Some(id.as_str()) {
                reached_known = true;
                break;
            }
            let object = info.object().map_err(unreadable)?;
            let time = object.time().map_err(unreadable)?.seconds;
            let subject = object
                .message()
                .map(|message| message.summary().to_string())
                .unwrap_or_default();
            commits.push(Commit {
                id,
                time,
                subject,
                changes: self.changes(&info)?,
            });
        }
        Ok(NewHistory {
            head: head_id,
            commits,
            rewritten: known.is_some() && !reached_known,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn git(folder: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(folder)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .expect("git runs");
        assert!(status.success(), "git {arguments:?}");
    }

    #[test]
    fn a_project_inside_a_repository_sees_its_own_files_and_their_renames() {
        let root = std::env::temp_dir().join(format!("cyclops-git-{}", std::process::id()));
        let project = root.join("app");
        fs::create_dir_all(project.join("src")).expect("folders");
        fs::write(project.join("src/a.ts"), "a\n").expect("a file");
        fs::write(root.join("outside.ts"), "b\n").expect("a file");
        git(&root, &["init", "-q"]);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "First"]);
        git(&root, &["mv", "app/src/a.ts", "app/src/b.ts"]);
        git(&root, &["commit", "-q", "-m", "Rename"]);

        let history = GitHistory::open(&project).expect("a repository");
        let all = history.read_since(None).expect("read");
        let first = all.commits.last().map(|commit| commit.id.clone());
        let since_first = history.read_since(first.as_deref()).expect("read");
        let gone = history
            .read_since(Some("0000000000000000000000000000000000000000"))
            .expect("read");
        fs::remove_dir_all(&root).expect("removed");

        let renamed = all.commits.first().expect("the newest commit");
        assert_eq!(
            renamed.changes,
            [FileChange {
                path: "src/b.ts".to_owned(),
                previous: Some("src/a.ts".to_owned()),
            }]
        );
        let added: Vec<&str> = all
            .commits
            .last()
            .expect("the first commit")
            .changes
            .iter()
            .map(|change| change.path.as_str())
            .collect();
        assert_eq!(added, ["src/a.ts"]);
        assert_eq!(since_first.commits.len(), 1);
        assert!(!since_first.rewritten);
        assert_eq!(gone.commits.len(), 2);
        assert!(gone.rewritten);
    }

    #[test]
    fn a_folder_outside_a_repository_has_no_history() {
        let folder = std::env::temp_dir().join(format!("cyclops-nogit-{}", std::process::id()));
        fs::create_dir_all(&folder).expect("a folder");
        let found = GitHistory::open(&folder).is_some();
        fs::remove_dir_all(&folder).expect("removed");
        assert!(!found || std::env::temp_dir().join(".git").exists());
    }
}
