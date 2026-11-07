//! What a finished analysis wants the person to know.

use crate::path::RelativePath;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Problem {
    NoSourceFiles,
    SyntaxError(RelativePath),
    NotUtf8(RelativePath),
    Unreadable(RelativePath),
    /// The cache could not be used, so everything was parsed again.
    CacheUnavailable(String),
    /// Precise analysis was asked for, but the settings turn it off.
    PreciseOff,
    /// An indexer could not run, so these languages kept the syntax layer's relations.
    PreciseSkipped {
        language: String,
        reason: String,
    },
    /// An indexer ran and failed.
    PreciseFailed {
        language: String,
        reason: String,
    },
}

impl Problem {
    fn path(&self) -> Option<&RelativePath> {
        match self {
            Self::NoSourceFiles
            | Self::CacheUnavailable(_)
            | Self::PreciseOff
            | Self::PreciseSkipped { .. }
            | Self::PreciseFailed { .. } => None,
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
            Self::CacheUnavailable(reason) => write!(f, "The cache cannot be used: {reason}"),
            Self::PreciseOff => f.write_str("Precise analysis is turned off for this project."),
            Self::PreciseSkipped { language, reason } => {
                write!(f, "Precise analysis skipped for {language}: {reason}.")
            }
            Self::PreciseFailed { language, reason } => {
                write!(f, "Precise analysis failed for {language}: {reason}")
            }
        }
    }
}
