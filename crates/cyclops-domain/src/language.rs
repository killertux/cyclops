//! The languages Cyclops understands.

use serde::{Deserialize, Serialize};

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LanguageId {
    Rust,
    TypeScript,
    JavaScript,
}

impl LanguageId {
    /// The name used in settings and on screen.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
        }
    }
}

impl LanguageId {
    /// Copies only match within one family, since other languages read differently.
    #[must_use]
    pub const fn family(self) -> u8 {
        match self {
            Self::Rust => 0,
            Self::TypeScript | Self::JavaScript => 1,
        }
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
