//! Builds the symbols of a project, as a language adapter places them. What differs
//! between languages is where things go. This is what stays the same.

use crate::facts::{FileFacts, SymbolFact};
use crate::path::RelativePath;
use crate::symbol::{CodeRole, Location, Span, Symbol, SymbolId, SymbolIds, SymbolKind};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct SymbolSet {
    ids: SymbolIds,
    symbols: Vec<Symbol>,
    /// Where each package and module is in `symbols`.
    containers: HashMap<SymbolId, usize>,
    /// Where every symbol is in `symbols`.
    positions: HashMap<SymbolId, usize>,
}

impl SymbolSet {
    /// Makes sure a package or a module exists. It is production code as soon as any file
    /// in it is.
    pub fn container(
        &mut self,
        id: &SymbolId,
        parent: Option<&SymbolId>,
        name: &str,
        kind: SymbolKind,
        role: CodeRole,
    ) {
        if let Some(index) = self.containers.get(id) {
            if let Some(symbol) = self.symbols.get_mut(*index)
                && role == CodeRole::Production
            {
                symbol.role = role;
            }
            return;
        }
        let unique = self.ids.unique(id.clone());
        self.containers.insert(id.clone(), self.symbols.len());
        self.positions.insert(unique.clone(), self.symbols.len());
        self.symbols
            .push(Symbol::container(unique, parent.cloned(), name, kind, role));
    }

    /// Says which file a module is written in.
    pub fn written_in(&mut self, module: &SymbolId, file: &FileFacts) {
        let symbol = self
            .containers
            .get(module)
            .and_then(|index| self.symbols.get_mut(*index));
        if let Some(symbol) = symbol {
            let lines = file.lines;
            symbol.location = Some(Location {
                file: file.path.clone(),
                span: Span {
                    start_line: 1,
                    end_line: lines.code + lines.comment + lines.blank,
                },
            });
            symbol.code_lines = lines.code;
        }
    }

    /// Adds what a file describes, and says the id it ended up with.
    pub fn add(
        &mut self,
        fact: &SymbolFact,
        id: SymbolId,
        parent: SymbolId,
        file: &RelativePath,
    ) -> SymbolId {
        // A field gives way to a method of the same name, as a getter has.
        let id = if fact.kind == SymbolKind::Field {
            self.ids.unique_as(id, "field")
        } else {
            self.ids.unique(id)
        };
        let symbol = Symbol::from_fact(fact, id.clone(), Some(parent), file);
        self.positions.insert(id.clone(), self.symbols.len());
        self.symbols.push(symbol);
        id
    }

    /// A symbol added so far.
    #[must_use]
    pub fn get(&self, id: &SymbolId) -> Option<&Symbol> {
        self.positions
            .get(id)
            .and_then(|index| self.symbols.get(*index))
    }

    /// Counts more lines of code for a symbol, such as an `impl` block of a type.
    pub fn add_lines(&mut self, id: &SymbolId, lines: u32) {
        let index = self.positions.get(id).copied();
        if let Some(symbol) = index.and_then(|index| self.symbols.get_mut(index)) {
            symbol.code_lines += lines;
        }
    }

    #[must_use]
    pub fn into_symbols(self) -> Vec<Symbol> {
        self.symbols
    }
}
