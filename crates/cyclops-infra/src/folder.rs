//! The `.cyclops/` folder of a project: its settings, and its cache.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FOLDER: &str = ".cyclops";
const CONFIG: &str = "config.toml";
/// Keeps the cache out of the repository, so that nobody has to edit their own ignores.
const IGNORE: &str = "cache/\n";

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

    /// Creates the folder and its `.gitignore`, if they are missing.
    ///
    /// # Errors
    ///
    /// Fails if they cannot be written.
    pub fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(&self.folder)?;
        let ignore = self.folder.join(".gitignore");
        if !ignore.exists() {
            fs::write(ignore, IGNORE)?;
        }
        Ok(())
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
