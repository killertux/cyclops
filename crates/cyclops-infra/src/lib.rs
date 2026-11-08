//! File access for Cyclops: the project's files, its `.cyclops/` folder, both settings
//! files, and the fact cache.

mod cache;
mod config;
mod export;
mod file_source;
mod folder;
mod git;
mod rules;
mod watch;

pub use cache::SqliteFactStore;
pub use config::{UserConfig, read_layer};
pub use export::{Export, ExportFormat};
pub use file_source::{ProjectFiles, ProjectNotFound};
pub use folder::ProjectFolder;
pub use git::GitHistory;
pub use rules::{save_alert, set_alert_enabled};
pub use watch::ProjectWatcher;
