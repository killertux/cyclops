//! Finding programs the way a shell does, on a search path.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// The programs an indexer may run: those on a search path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tools {
    path: OsString,
}

#[cfg(unix)]
fn is_program(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|found| found.is_file() && found.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_program(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|found| found.is_file())
}

impl Tools {
    /// The programs on `path`, written the way the `PATH` variable is. Without one, there
    /// are none.
    #[must_use]
    pub fn new(path: Option<&str>) -> Self {
        Self {
            path: path.map(OsString::from).unwrap_or_default(),
        }
    }

    /// The search path, for the processes an indexer starts.
    #[must_use]
    pub fn search_path(&self) -> &OsString {
        &self.path
    }

    /// Where a program is, if it is on the search path.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        let names = if cfg!(windows) {
            vec![format!("{name}.exe"), format!("{name}.cmd")]
        } else {
            vec![name.to_owned()]
        };
        env::split_paths(&self.path)
            .filter(|folder| !folder.as_os_str().is_empty())
            .flat_map(|folder| names.iter().map(move |name| folder.join(name)))
            .find(|candidate| is_program(candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = env::temp_dir().join(format!("cyclops-tools-{}-{name}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[cfg(unix)]
    fn program(folder: &Path, name: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = folder.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn finds_a_program_in_the_first_folder_that_has_it() {
        let first = folder("first");
        let second = folder("second");
        program(&second, "indexer");
        let path = env::join_paths([&first, &second]).unwrap();
        let tools = Tools::new(path.to_str());
        assert_eq!(tools.find("indexer"), Some(second.join("indexer")));
        program(&first, "indexer");
        assert_eq!(tools.find("indexer"), Some(first.join("indexer")));
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_run_is_not_a_program() {
        let only = folder("not-a-program");
        fs::write(only.join("notes"), "text").unwrap();
        let tools = Tools::new(only.to_str());
        assert_eq!(tools.find("notes"), None);
    }

    #[test]
    fn without_a_search_path_there_is_nothing() {
        assert_eq!(Tools::new(None).find("sh"), None);
    }
}
