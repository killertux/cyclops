//! What a finished analysis wants the person to know.

use crate::path::RelativePath;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Problem {
    NoSourceFiles,
    SyntaxError(RelativePath),
    NotUtf8(RelativePath),
    Unreadable(RelativePath),
}

impl Problem {
    fn path(&self) -> Option<&RelativePath> {
        match self {
            Self::NoSourceFiles => None,
            Self::SyntaxError(path) | Self::NotUtf8(path) | Self::Unreadable(path) => Some(path),
        }
    }

    /// Orders problems by the file they are about.
    pub fn sort(problems: &mut [Self]) {
        problems.sort_by(|a, b| a.path().cmp(&b.path()).then_with(|| a.cmp(b)));
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSourceFiles => f.write_str("No source files found."),
            Self::SyntaxError(path) => write!(f, "{path}: syntax error"),
            Self::NotUtf8(path) => write!(f, "{path}: not valid UTF-8"),
            Self::Unreadable(path) => write!(f, "{path}: cannot be read"),
        }
    }
}
