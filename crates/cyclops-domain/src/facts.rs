//! Facts: what a language adapter extracts from one file. Nothing here depends on any
//! other file, so facts can be worked out in parallel and cached by content.

use serde::{Deserialize, Serialize};

use crate::language::LanguageId;
use crate::path::RelativePath;
use crate::symbol::{CodeRole, Span, SymbolKind, Visibility};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// The modules this file declares as test code, each living in a file of its own,
    /// as a path of names from this file's module.
    pub test_modules: Vec<Vec<String>>,
    /// Blocks that add members to a type written elsewhere, such as Rust's `impl`.
    pub extensions: Vec<Extension>,
    /// What this file makes available from other modules, such as a barrel file's
    /// `export { a } from "./a"`.
    pub exports: Vec<Export>,
}

/// A block that adds to a type, wherever the type is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extension {
    /// The type, as written.
    pub owner: Vec<String>,
    /// The symbol around the block, if any.
    pub parent: Option<usize>,
    /// The interface it implements, if it does.
    pub implements: Option<Vec<String>>,
    pub code_lines: u32,
}

/// A name a module passes on from another module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export {
    /// The name it is exported as, or nothing for all of them.
    pub name: Option<String>,
    /// The name in the other module.
    pub original: Option<String>,
    /// The other module, as written.
    pub from: String,
}

/// Each line of a file is exactly one of these.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub member: MemberFacts,
    pub references: Vec<Reference>,
}

/// What a member says about how it belongs to its type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberFacts {
    /// It works on an instance: not a constructor, a static method or an associated
    /// function.
    pub instance: bool,
    /// It serves an interface the type implements, rather than the type itself.
    pub for_interface: bool,
}

/// A name a file brings into scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// The name used in the file, or nothing when it brings in everything at `path`.
    pub alias: Option<String>,
    /// The path of the name. For a module written as a string, as in JavaScript, the
    /// name inside that module, or nothing for the module itself.
    pub path: Vec<String>,
    /// The module, when it is written as a string.
    pub from: Option<String>,
    /// The symbol whose scope the import is in, or nothing for the whole file.
    pub scope: Option<usize>,
}

/// Something a symbol mentions, before it is known what that is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub kind: ReferenceKind,
    /// The name, as written, such as `crate::billing::Invoice`.
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceKind {
    /// A type, such as in a parameter, a field or an expression.
    Type,
    /// A function called by its path.
    Call,
    /// A method called on something whose type is not known.
    MethodCall,
    /// A method called on `self` or `this`.
    SelfCall,
    /// An interface this type implements.
    Implements,
    /// A type this one extends.
    Inherits,
}

/// What the body of a function contains.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyFacts {
    pub decisions: Vec<DecisionPoint>,
    pub parameters: u32,
    /// The depth of the deepest control structure.
    pub max_nesting: u32,
    /// The fields it reads or writes through `self` or `this`.
    pub fields_used: Vec<String>,
    /// The methods it calls through `self` or `this`.
    pub methods_called: Vec<String>,
}

/// One thing in a function that adds to its complexity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionPoint {
    pub kind: DecisionKind,
    pub line: u32,
    /// How deep it is, for cognitive complexity. Unlike `BodyFacts::max_nesting`, this
    /// also counts closures.
    pub nesting: u32,
}

/// What each language's constructs map to. Complexity is written once, against these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// A condition inside an expression.
    Ternary,
    Catch,
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
            | Self::Ternary
            | Self::Catch
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
            Self::If | Self::Loop | Self::BareLoop | Self::Match | Self::Ternary | Self::Catch => {
                1 + nesting
            }
            Self::ElseIf | Self::Else | Self::LogicalRun | Self::Recursion | Self::LabeledJump => 1,
            Self::MatchArm | Self::Guard | Self::Shortcut | Self::LogicalOperator => 0,
        }
    }
}

/// Parsers recurse, so code nested deeply enough would overflow the stack and take the
/// whole analysis down. No person writes code near this deep.
pub const MAX_BRACKET_DEPTH: usize = 256;

/// Whether code is too deep to hand to a parser. It does not skip strings or comments,
/// which only matters to text with hundreds of unmatched brackets in a row.
#[must_use]
pub fn is_too_deep(text: &str) -> bool {
    let mut depth = 0_usize;
    for byte in text.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth > MAX_BRACKET_DEPTH {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_code_is_spotted_even_after_too_many_closing_brackets() {
        let open = "(".repeat(MAX_BRACKET_DEPTH + 1);
        assert!(is_too_deep(&open));
        assert!(is_too_deep(&format!("}}}}}}{open}")));
    }

    #[test]
    fn reasonable_code_is_not_too_deep() {
        let open = "(".repeat(MAX_BRACKET_DEPTH);
        assert!(!is_too_deep(&open));
        assert!(!is_too_deep(&"()".repeat(10_000)));
    }
}
