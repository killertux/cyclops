//! The `.cyclops/` folder of a project: its settings, and its cache.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::workspace::TakenByWorkspace;

const FOLDER: &str = ".cyclops";
const CONFIG: &str = "config.toml";
/// Keeps the cache and the log out of the repository, so that nobody has to edit their
/// own ignores.
const IGNORE: [&str; 2] = ["cache/", "logs/"];

#[derive(Debug, Clone)]
pub struct ProjectFolder {
    folder: PathBuf,
}

impl ProjectFolder {
    #[must_use]
    pub fn of(project: &Path) -> Self {
        Self {
            folder: project.join(FOLDER),
        }
    }

    /// The Cargo workspace that would take this folder as one of its crates, and stop
    /// building, if there is one.
    ///
    /// # Errors
    ///
    /// Fails if the project's real path cannot be found.
    pub fn taken_by_workspace(&self) -> io::Result<Option<TakenByWorkspace>> {
        let Some(project) = self.folder.parent() else {
            return Ok(None);
        };
        Ok(crate::workspace::taking(
            &fs::canonicalize(project)?,
            FOLDER,
        ))
    }

    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.folder.join(CONFIG)
    }

    #[must_use]
    pub fn cache_path(&self) -> PathBuf {
        self.folder.join("cache").join("cyclops.db")
    }

    #[must_use]
    pub fn has_config(&self) -> bool {
        self.config_path().exists()
    }

    #[must_use]
    pub fn log_path(&self) -> PathBuf {
        self.folder.join("logs").join("cyclops.log")
    }

    /// Creates the folder and its `.gitignore`, if they are missing. A `.gitignore` made
    /// before the log was kept gets its line added.
    ///
    /// # Errors
    ///
    /// Fails if they cannot be written.
    pub fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(&self.folder)?;
        let ignore = self.folder.join(".gitignore");
        let mut text = match fs::read_to_string(&ignore) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        let missing: Vec<&str> = IGNORE
            .into_iter()
            .filter(|line| !text.lines().any(|known| known.trim() == *line))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        for line in missing {
            text.push_str(line);
            text.push('\n');
        }
        fs::write(ignore, text)
    }

    /// Writes the project's settings. It never overwrites them.
    ///
    /// # Errors
    ///
    /// Fails if the file exists, or cannot be written.
    pub fn create_config(&self, text: &str) -> io::Result<()> {
        self.prepare()?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.config_path())
            .and_then(|mut file| io::Write::write_all(&mut file, text.as_bytes()))
    }
}
