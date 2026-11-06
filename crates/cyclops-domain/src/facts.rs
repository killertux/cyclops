//! Facts: what a language adapter extracts from one file. Nothing here depends on any
//! other file, so facts can be worked out in parallel and cached by content.

use crate::language::LanguageId;
use crate::path::RelativePath;
use crate::symbol::{CodeRole, Span, SymbolKind, Visibility};

#[derive(Debug, Clone, PartialEq)]
pub struct FileFacts {
    pub path: RelativePath,
    pub language: LanguageId,
    pub role: CodeRole,
    /// Every line of the file.
    pub lines: LineCounts,
    /// The part of `lines` that is test code.
    pub test_lines: LineCounts,
    pub symbols: Vec<SymbolFact>,
    pub imports: Vec<Import>,
    /// Whether part of the file could not be understood.
    pub syntax_error: bool,
}

/// Each line of a file is exactly one of these.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineCounts {
    pub code: u32,
    pub comment: u32,
    pub blank: u32,
}

impl LineCounts {
    /// What is left after taking `other` out.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self {
            code: self.code.saturating_sub(other.code),
            comment: self.comment.saturating_sub(other.comment),
            blank: self.blank.saturating_sub(other.blank),
        }
    }

    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            code: self.code + other.code,
            comment: self.comment + other.comment,
            blank: self.blank + other.blank,
        }
    }
}

/// A symbol as one file sees it, before the project gives it a full name.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolFact {
    pub name: String,
    /// The index in `FileFacts::symbols` of the symbol around this one. It always comes
    /// first. Without one, the symbol sits directly in the file.
    pub parent: Option<usize>,
    /// The path of the type this member belongs to, when that type may be in another
    /// file, as written in the code.
    pub owner: Option<Vec<String>>,
    pub kind: SymbolKind,
    pub role: CodeRole,
    pub visibility: Visibility,
    pub span: Span,
    pub code_lines: u32,
    pub body: Option<BodyFacts>,
}

/// A name a file brings into scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The name used in the file, or nothing when it brings in everything at `path`.
    pub alias: Option<String>,
    pub path: Vec<String>,
}

/// What the body of a function contains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BodyFacts {
    pub decisions: Vec<DecisionPoint>,
    pub parameters: u32,
    /// The depth of the deepest control structure.
    pub max_nesting: u32,
}

/// One thing in a function that adds to its complexity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionPoint {
    pub kind: DecisionKind,
    pub line: u32,
    /// How deep it is, for cognitive complexity. Unlike `BodyFacts::max_nesting`, this
    /// also counts closures.
    pub nesting: u32,
}

/// What each language's constructs map to. Complexity is written once, against these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKind {
    If,
    ElseIf,
    Else,
    /// A loop with an exit test.
    Loop,
    /// A loop with no exit test, such as `loop` in Rust.
    BareLoop,
    /// A whole `match` or `switch`.
    Match,
    /// One more way out of a `match` or `switch`.
    MatchArm,
    /// A condition on a single arm.
    Guard,
    /// A shorthand that can skip code and reads as straight code, such as `?`.
    Shortcut,
    /// One logical operator.
    LogicalOperator,
    /// A run of the same logical operator.
    LogicalRun,
    Recursion,
    LabeledJump,
}

impl DecisionKind {
    /// How many paths it adds, for cyclomatic complexity.
    #[must_use]
    pub const fn paths(self) -> u32 {
        match self {
            Self::If
            | Self::ElseIf
            | Self::Loop
            | Self::MatchArm
            | Self::Guard
            | Self::Shortcut
            | Self::LogicalOperator => 1,
            Self::Else
            | Self::BareLoop
            | Self::Match
            | Self::LogicalRun
            | Self::Recursion
            | Self::LabeledJump => 0,
        }
    }

    /// How many points it adds at a depth, for cognitive complexity.
    #[must_use]
    pub const fn points(self, nesting: u32) -> u32 {
        match self {
            Self::If | Self::Loop | Self::BareLoop | Self::Match => 1 + nesting,
            Self::ElseIf | Self::Else | Self::LogicalRun | Self::Recursion | Self::LabeledJump => 1,
            Self::MatchArm | Self::Guard | Self::Shortcut | Self::LogicalOperator => 0,
        }
    }
}
