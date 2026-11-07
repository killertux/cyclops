//! Relations: how symbols connect, and how sure Cyclops is of it.

use crate::symbol::SymbolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RelationKind {
    Implements,
    Inherits,
    Composes,
    Uses,
    Calls,
    Imports,
}

impl RelationKind {
    pub const ALL: [Self; 6] = [
        Self::Implements,
        Self::Inherits,
        Self::Composes,
        Self::Uses,
        Self::Calls,
        Self::Imports,
    ];

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Imports => "imports",
            Self::Implements => "implements",
            Self::Inherits => "inherits",
            Self::Composes => "composes",
            Self::Uses => "uses",
            Self::Calls => "calls",
        }
    }
}

/// How much a relation can be trusted. Earlier is surer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Confidence {
    /// Written out in the code, or proven.
    Exact,
    /// Matched by name, and only one symbol fits.
    Inferred,
    /// Matched by name, and several symbols fit.
    Ambiguous,
}

impl Confidence {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Inferred => "inferred",
            Self::Ambiguous => "ambiguous",
        }
    }

    /// The confidence of a match among this many candidates.
    #[must_use]
    pub const fn of_candidates(count: usize) -> Self {
        if count == 1 {
            Self::Inferred
        } else {
            Self::Ambiguous
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Relation {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: RelationKind,
    pub confidence: Confidence,
}
