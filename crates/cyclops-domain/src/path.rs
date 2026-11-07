//! Paths inside a project.

use serde::{Deserialize, Serialize};

use std::fmt;

/// A path relative to the project root, always written with forward slashes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RelativePath(String);

impl RelativePath {
    #[must_use]
    pub fn new(path: &str) -> Self {
        Self(path.replace('\\', "/"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
