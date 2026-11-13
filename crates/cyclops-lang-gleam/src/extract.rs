//! Extracts the facts of one Gleam file.

use crate::language;
use crate::symbols::Extractor;
use crate::tree::{each_node, end, start};
use cyclops_domain::facts::{FileFacts, LineCounts, Token, is_too_deep};
use cyclops_domain::language::LanguageId;
use cyclops_domain::lines::LineMap;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::CodeRole;
use tree_sitter::{Node, Parser, Tree};

/// The kinds of comment: `//`, `///` before a definition, and `////` for the module.
const COMMENTS: [&str; 3] = ["comment", "statement_comment", "module_comment"];

pub(crate) fn extract(path: &RelativePath, text: &str) -> FileFacts {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let role = language::role(path);
    let tree = if is_too_deep(text) { None } else { parse(text) };
    let Some(tree) = tree else {
        return unparsed(path, text, role);
    };
    let root = tree.root_node();
    let comments = comments(root);
    let mut lines = LineMap::new(text);
    for (from, to) in &comments {
        lines.mark_comment(*from, *to);
    }
    lines.mark_code_outside(text, &comments);
    let mut extractor = Extractor::new(&lines, text, role);
    extractor.file(root);
    let (symbols, imports) = extractor.into_parts();
    let counts = lines.counts();
    FileFacts {
        path: path.clone(),
        language: LanguageId::Gleam,
        role,
        lines: counts,
        test_lines: test_lines(role, counts),
        symbols,
        imports,
        syntax_error: root.has_error(),
        test_modules: Vec::new(),
        extensions: Vec::new(),
        exports: Vec::new(),
        tokens: tokens(root, &lines, role == CodeRole::Test),
    }
}

/// Parses a file. A grammar the library cannot load leaves every file unparsed, which
/// shows as a syntax error in each of them rather than as a crash.
fn parse(text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_gleam::LANGUAGE.into())
        .is_ok()
        .then(|| parser.parse(text, None))
        .flatten()
}

/// A file that could not be parsed at all still has lines.
fn unparsed(path: &RelativePath, text: &str, role: CodeRole) -> FileFacts {
    let counts = LineMap::unparsed(text).counts();
    FileFacts {
        path: path.clone(),
        language: LanguageId::Gleam,
        role,
        lines: counts,
        test_lines: test_lines(role, counts),
        symbols: Vec::new(),
        imports: Vec::new(),
        syntax_error: true,
        test_modules: Vec::new(),
        extensions: Vec::new(),
        exports: Vec::new(),
        tokens: Vec::new(),
    }
}

fn test_lines(role: CodeRole, counts: LineCounts) -> LineCounts {
    match role {
        CodeRole::Test => counts,
        CodeRole::Production => LineCounts::default(),
    }
}

/// Where each comment is, in order.
fn comments(root: Node<'_>) -> Vec<(u32, u32)> {
    let mut found = Vec::new();
    each_node(root, |node| {
        if COMMENTS.contains(&node.kind()) {
            found.push((start(node), end(node)));
        }
    });
    found
}

/// The code as tokens, with names and literal values made alike, for finding copies.
fn tokens(root: Node<'_>, lines: &LineMap, test: bool) -> Vec<Token> {
    let mut found = Vec::new();
    each_node(root, |node| {
        let is_token = node.child_count() == 0 && !node.is_missing();
        if is_token && !COMMENTS.contains(&node.kind()) {
            found.push(Token {
                kind: token_kind(node),
                line: lines.line_of(start(node)),
                test,
            });
        }
    });
    found
}

fn token_kind(node: Node<'_>) -> u16 {
    match node.kind() {
        "identifier" | "label" | "type_identifier" | "constructor_name" | "discard" | "module" => {
            Token::NAME
        }
        "integer" | "float" | "quoted_content" | "bit_string_segment_option_unit" => Token::LITERAL,
        _ => Token::FIRST_OTHER.saturating_add(node.kind_id()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyclops_domain::symbol::SymbolKind;

    fn facts_of(path: &str, text: &str) -> FileFacts {
        extract(&RelativePath::new(path), text)
    }

    fn names(facts: &FileFacts) -> Vec<&str> {
        facts
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect()
    }

    #[test]
    fn what_comes_before_a_syntax_error_is_kept() {
        let facts = facts_of(
            "src/bad.gleam",
            "pub fn kept() -> Int {\n  1\n}\n\npub fn broken( {\n",
        );
        assert!(facts.syntax_error);
        assert_eq!(names(&facts), ["kept"]);
    }

    #[test]
    fn healthy_code_has_no_syntax_error() {
        let facts = facts_of("src/a.gleam", "pub fn f() -> Int {\n  1\n}\n");
        assert!(!facts.syntax_error);
        assert_eq!(facts.language, LanguageId::Gleam);
    }

    #[test]
    fn every_kind_of_comment_is_a_comment() {
        let facts = facts_of("src/a.gleam", "//// m\n/// d\n// c\n\nfn f() { 1 }\n");
        assert_eq!(
            facts.lines,
            LineCounts {
                code: 1,
                comment: 3,
                blank: 1,
            }
        );
    }

    #[test]
    fn a_comment_in_a_string_is_not_a_comment() {
        let facts = facts_of("src/a.gleam", "const a = \"// b\"\n");
        assert_eq!(facts.lines.comment, 0);
        assert_eq!(facts.lines.code, 1);
    }

    #[test]
    fn code_too_deep_for_the_parser_is_a_syntax_error_and_not_a_crash() {
        let depth = 20_000;
        let text = format!("const a = {}1{}\n", "[".repeat(depth), "]".repeat(depth));
        let facts = facts_of("src/a.gleam", &text);
        assert!(facts.syntax_error);
        assert_eq!(facts.symbols, []);
    }

    #[test]
    fn a_renamed_copy_has_the_same_tokens() {
        let kinds = |text: &str| {
            facts_of("src/a.gleam", text)
                .tokens
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>()
        };
        let original = kinds("fn a(b: Int) { b + 1 }");
        assert_eq!(original, kinds("fn q(r: Int) { r + 2 }"));
        assert_ne!(original, kinds("fn a(b: Int) { b - 1 }"));
    }

    #[test]
    fn a_test_file_is_test_code_with_all_it_holds() {
        let facts = facts_of("test/a_test.gleam", "pub fn f_test() { Nil }\n");
        assert_eq!(facts.role, CodeRole::Test);
        assert_eq!(facts.test_lines, facts.lines);
        let f = facts.symbols.first().expect("a symbol");
        assert_eq!(f.role, CodeRole::Test);
        assert_eq!(f.kind, SymbolKind::Function);
    }
}
