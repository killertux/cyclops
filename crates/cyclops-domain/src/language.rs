//! The languages Cyclops understands.

use serde::{Deserialize, Serialize};

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LanguageId {
    Rust,
    TypeScript,
    JavaScript,
    Gleam,
    Elixir,
    Python,
}

impl LanguageId {
    /// Every language, in the order of their names.
    pub const ALL: [Self; 6] = [
        Self::Elixir,
        Self::Gleam,
        Self::JavaScript,
        Self::Python,
        Self::Rust,
        Self::TypeScript,
    ];

    /// The name used in settings and on screen.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Gleam => "gleam",
            Self::Elixir => "elixir",
            Self::Python => "python",
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
            Self::Gleam => 2,
            Self::Elixir => 3,
            Self::Python => 4,
        }
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
