//! Symbols: the packages, modules, types and members of a project.

use serde::{Deserialize, Serialize};

use crate::facts::{BodyFacts, SymbolFact};
use crate::path::RelativePath;
use std::collections::HashMap;
use std::fmt;

/// The name of a symbol, unique within a project, such as `shop::orders::Order::total`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(String);

impl SymbolId {
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self(id.to_owned())
    }

    /// The id of something named `name` directly inside this symbol.
    #[must_use]
    pub fn child(&self, name: &str) -> Self {
        Self(format!("{}::{name}", self.0))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Hands out ids that are unique in a project. Two symbols can share a name, such as
/// the `fmt` of `Debug` and of `Display` on one type, or two `cfg` variants of a
/// function. The first keeps the name, and the next ones are `name#2`, `name#3`.
#[derive(Debug, Default)]
pub struct SymbolIds {
    seen: HashMap<SymbolId, u32>,
}

impl SymbolIds {
    pub fn unique(&mut self, id: SymbolId) -> SymbolId {
        let count = self.seen.entry(id.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            id
        } else {
            SymbolId(format!("{id}#{count}"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Package,
    Module,
    Struct,
    Class,
    Enum,
    Interface,
    Alias,
    Function,
    Method,
    Field,
    Constant,
}

impl SymbolKind {
    /// The name used in filters and on screen.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::Module => "module",
            Self::Struct => "struct",
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Alias => "alias",
            Self::Function => "function",
            Self::Method => "method",
            Self::Field => "field",
            Self::Constant => "constant",
        }
    }

    #[must_use]
    pub const fn is_type(self) -> bool {
        matches!(
            self,
            Self::Struct | Self::Class | Self::Enum | Self::Interface | Self::Alias
        )
    }
}

/// Whether code is part of the product or only tests it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeRole {
    Production,
    Test,
}

impl CodeRole {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Test => "test",
        }
    }
}

/// Who can reach a symbol. `Restricted` is anything between the two ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Restricted,
    Private,
}

impl Visibility {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Restricted => "restricted",
            Self::Private => "private",
        }
    }
}

/// The lines a symbol covers, counted from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start_line: u32,
    pub end_line: u32,
}

/// Where a symbol is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: RelativePath,
    pub span: Span,
}

/// A symbol after linking, with its place in the project.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub id: SymbolId,
    pub parent: Option<SymbolId>,
    pub name: String,
    pub kind: SymbolKind,
    pub role: CodeRole,
    pub visibility: Visibility,
    /// Where it is written. A package, or a folder that is a module, has no such place.
    pub location: Option<Location>,
    /// Its lines of code, leaving out blank lines and comments.
    pub code_lines: u32,
    pub body: Option<BodyFacts>,
}

impl Symbol {
    /// A symbol that is not written in any one file.
    #[must_use]
    pub fn container(
        id: SymbolId,
        parent: Option<SymbolId>,
        name: &str,
        kind: SymbolKind,
        role: CodeRole,
    ) -> Self {
        Self {
            id,
            parent,
            name: name.to_owned(),
            kind,
            role,
            visibility: Visibility::Public,
            location: None,
            code_lines: 0,
            body: None,
        }
    }

    /// The symbol an adapter described, in its place in the project.
    #[must_use]
    pub fn from_fact(
        fact: &SymbolFact,
        id: SymbolId,
        parent: Option<SymbolId>,
        file: &RelativePath,
    ) -> Self {
        Self {
            id,
            parent,
            name: fact.name.clone(),
            kind: fact.kind,
            role: fact.role,
            visibility: fact.visibility,
            location: Some(Location {
                file: file.clone(),
                span: fact.span,
            }),
            code_lines: fact.code_lines,
            body: fact.body.clone(),
        }
    }
}
