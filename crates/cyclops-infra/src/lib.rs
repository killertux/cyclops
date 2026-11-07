//! File access for Cyclops: the project's files, its `.cyclops/` folder, both settings
//! files, and the fact cache.

mod cache;
mod config;
mod file_source;
mod folder;

pub use cache::SqliteFactStore;
pub use config::{UserConfig, read_layer};
pub use file_source::{ProjectFiles, ProjectNotFound};
pub use folder::ProjectFolder;
