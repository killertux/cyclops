//! Reading a SCIP index into what the analysis needs: where names are defined and used.

use cyclops_application::ports::{
    IndexError, IndexedDefinition, IndexedDocument, IndexedReference, SemanticIndex,
};
use cyclops_domain::path::RelativePath;
use protobuf::Message;
use scip::symbol::{format_symbol, is_local_symbol, parse_symbol};
use scip::types::descriptor::Suffix;
use scip::types::{Descriptor, Index, Symbol, SymbolRole};

/// Whether a name can be a symbol of the project. Namespaces, parameters and type
/// parameters cannot.
fn names_a_symbol(descriptor: &Descriptor) -> bool {
    matches!(
        descriptor.suffix.enum_value(),
        Ok(Suffix::Type | Suffix::Term | Suffix::Method | Suffix::Macro)
    )
}

/// The name as the code writes it. A constructor is `<constructor>` to SCIP.
fn plain_name(name: &str) -> String {
    match name {
        "<constructor>" => "constructor".to_owned(),
        other => other.to_owned(),
    }
}

/// The key of what a name belongs to, such as the class of a constructor.
fn owner_of(mut symbol: Symbol) -> Option<String> {
    symbol.descriptors.pop()?;
    names_a_symbol(symbol.descriptors.last()?).then(|| format_symbol(symbol))
}

/// Reads an index that an indexer wrote.
///
/// # Errors
///
/// Fails if it is not a SCIP index.
pub fn read_index(bytes: &[u8]) -> Result<SemanticIndex, IndexError> {
    read_rooted(bytes).map(|(index, _)| index)
}

/// Reads an index, with the folder its paths are relative to, as the indexer wrote it:
/// a `file://` URI.
///
/// # Errors
///
/// Fails if it is not a SCIP index.
pub fn read_rooted(bytes: &[u8]) -> Result<(SemanticIndex, String), IndexError> {
    let index = Index::parse_from_bytes(bytes)
        .map_err(|error| IndexError::Failed(format!("the index cannot be read: {error}")))?;
    let root = index
        .metadata
        .as_ref()
        .map(|metadata| metadata.project_root.clone())
        .unwrap_or_default();
    let definition = SymbolRole::Definition as i32;
    let documents = index
        .documents
        .iter()
        .map(|document| {
            let mut definitions = Vec::new();
            let mut references = Vec::new();
            for occurrence in &document.occurrences {
                if is_local_symbol(&occurrence.symbol) {
                    continue;
                }
                let line = occurrence
                    .range
                    .first()
                    .and_then(|line| u32::try_from(*line).ok())
                    .map(|line| line + 1);
                let Some(line) = line else {
                    continue;
                };
                let Ok(symbol) = parse_symbol(&occurrence.symbol) else {
                    continue;
                };
                let Some(name) = symbol
                    .descriptors
                    .last()
                    .filter(|last| names_a_symbol(last))
                    .map(|last| plain_name(&last.name))
                else {
                    continue;
                };
                if occurrence.symbol_roles & definition == 0 {
                    references.push(IndexedReference {
                        line,
                        key: occurrence.symbol.clone(),
                        owner: owner_of(symbol),
                    });
                } else {
                    definitions.push(IndexedDefinition {
                        line,
                        name,
                        key: occurrence.symbol.clone(),
                    });
                }
            }
            IndexedDocument {
                path: RelativePath::new(&document.relative_path),
                definitions,
                references,
            }
        })
        .collect();
    Ok((SemanticIndex { documents }, root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use scip::types::{Document, Occurrence};

    fn occurrence(line: i32, symbol: &str, defines: bool) -> Occurrence {
        Occurrence {
            range: vec![line, 4, 9],
            symbol: symbol.to_owned(),
            symbol_roles: if defines {
                SymbolRole::Definition as i32
            } else {
                0
            },
            ..Occurrence::default()
        }
    }

    fn index_of(occurrences: Vec<Occurrence>) -> SemanticIndex {
        let index = Index {
            documents: vec![Document {
                relative_path: "src/orders.ts".to_owned(),
                occurrences,
                ..Document::default()
            }],
            ..Index::default()
        };
        read_index(&index.write_to_bytes().unwrap()).unwrap()
    }

    const TOTAL: &str = "scip-typescript npm shop 0.1.0 src/`billing.ts`/Invoice#total().";
    const MAKE: &str = "scip-typescript npm shop 0.1.0 src/`billing.ts`/Invoice#`<constructor>`().";

    #[test]
    fn definitions_and_references_keep_their_lines_from_one() {
        let index = index_of(vec![
            occurrence(8, TOTAL, true),
            occurrence(14, TOTAL, false),
        ]);
        let document = index.documents.first().unwrap();
        assert_eq!(document.path, RelativePath::new("src/orders.ts"));
        assert_eq!(
            document.definitions,
            vec![IndexedDefinition {
                line: 9,
                name: "total".to_owned(),
                key: TOTAL.to_owned(),
            }]
        );
        let reference = document.references.first().unwrap();
        assert_eq!((reference.line, reference.key.as_str()), (15, TOTAL));
    }

    #[test]
    fn a_constructor_belongs_to_its_class() {
        let index = index_of(vec![occurrence(3, MAKE, false), occurrence(2, MAKE, true)]);
        let document = index.documents.first().unwrap();
        let reference = document.references.first().unwrap();
        assert_eq!(
            reference.owner.as_deref(),
            Some("scip-typescript npm shop 0.1.0 src/`billing.ts`/Invoice#")
        );
        assert_eq!(document.definitions.first().unwrap().name, "constructor");
    }

    #[test]
    fn locals_parameters_and_namespaces_are_not_symbols() {
        let index = index_of(vec![
            occurrence(1, "local 4", false),
            occurrence(2, &format!("{MAKE}(item)"), false),
            occurrence(3, "rust-analyzer cargo shop 0.1.0 billing/", false),
        ]);
        let document = index.documents.first().unwrap();
        assert_eq!(document.references, Vec::new());
        assert_eq!(document.definitions, Vec::new());
    }

    #[test]
    fn anything_else_is_not_an_index() {
        assert!(read_index(b"not an index at all").is_err());
    }
}
