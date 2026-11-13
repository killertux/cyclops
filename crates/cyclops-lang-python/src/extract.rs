//! Extracts the facts of one Python file.

use crate::language;
use crate::symbols::Extractor;
use crate::tree::{each_node, end, named_children, start};
use cyclops_domain::facts::{FileFacts, LineCounts, Token, is_too_deep};
use cyclops_domain::language::LanguageId;
use cyclops_domain::lines::LineMap;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::CodeRole;
use tree_sitter::{Node, Parser, Tree};

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
        language: LanguageId::Python,
        role,
        lines: counts,
        test_lines: test_lines(role, counts),
        symbols,
        imports,
        syntax_error: root.has_error(),
        test_modules: Vec::new(),
        extensions: Vec::new(),
        exports: Vec::new(),
        tokens: tokens(root, &lines, role == CodeRole::Test, &comments),
    }
}

/// Parses a file. A grammar the library cannot load leaves every file unparsed, which
/// shows as a syntax error in each of them rather than as a crash.
fn parse(text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .is_ok()
        .then(|| parser.parse(text, None))
        .flatten()
}

/// A file that could not be parsed at all still has lines.
fn unparsed(path: &RelativePath, text: &str, role: CodeRole) -> FileFacts {
    let counts = LineMap::unparsed(text).counts();
    FileFacts {
        path: path.clone(),
        language: LanguageId::Python,
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

/// The docstring of a module, a class or a function: a string standing alone as the
/// first statement of its body.
pub(crate) fn docstring(body: Node<'_>) -> Option<Node<'_>> {
    let first = named_children(body)
        .into_iter()
        .find(|child| child.kind() != "comment")?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let children = named_children(first);
    match children.as_slice() {
        [only] if matches!(only.kind(), "string" | "concatenated_string") => Some(first),
        _ => None,
    }
}

/// Where each comment is, in order: `#` comments, and docstrings, as a Rust doc comment
/// is one.
fn comments(root: Node<'_>) -> Vec<(u32, u32)> {
    let mut found = Vec::new();
    each_node(root, |node| {
        let body = match node.kind() {
            "module" => Some(node),
            "class_definition" | "function_definition" => node.child_by_field_name("body"),
            _ => None,
        };
        if let Some(docstring) = body.and_then(docstring) {
            found.push((start(docstring), end(docstring)));
        }
        if node.kind() == "comment" {
            found.push((start(node), end(node)));
        }
    });
    found.sort_unstable();
    found
}

/// The code as tokens, with names and literal values made alike, for finding copies.
fn tokens(root: Node<'_>, lines: &LineMap, test: bool, comments: &[(u32, u32)]) -> Vec<Token> {
    let mut found = Vec::new();
    each_node(root, |node| {
        let at = start(node);
        let in_comment = comments.iter().any(|(from, to)| *from <= at && at < *to);
        if node.child_count() == 0
            && !node.is_missing()
            && !in_comment
            && node.end_byte() > node.start_byte()
        {
            found.push(Token {
                kind: token_kind(node),
                line: lines.line_of(at),
                test,
            });
        }
    });
    found
}

fn token_kind(node: Node<'_>) -> u16 {
    match node.kind() {
        "identifier" => Token::NAME,
        "integer" | "float" | "string_content" | "true" | "false" | "none" => Token::LITERAL,
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

    #[test]
    fn docstrings_are_comments() {
        let facts = facts_of(
            "src/a.py",
            "\"\"\"Module.\"\"\"\n# c\n\ndef f():\n    \"\"\"a\n    b\"\"\"\n    g()\n",
        );
        assert_eq!(
            facts.lines,
            LineCounts {
                code: 2,
                comment: 4,
                blank: 1,
            }
        );
    }

    #[test]
    fn a_string_that_does_something_is_code() {
        let facts = facts_of("src/a.py", "def f():\n    x = \"\"\"a\"\"\"\n");
        assert_eq!(facts.lines.comment, 0);
        assert_eq!(facts.lines.code, 2);
    }

    #[test]
    fn what_comes_before_a_syntax_error_is_kept() {
        let facts = facts_of("py/bad.py", "def kept():\n    return 1\n\n\ndef broken(:\n");
        assert!(facts.syntax_error);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "kept"));
    }

    #[test]
    fn code_too_deep_for_the_parser_is_a_syntax_error_and_not_a_crash() {
        let depth = 20_000;
        let text = format!("x = {}1{}\n", "[".repeat(depth), "]".repeat(depth));
        let facts = facts_of("src/a.py", &text);
        assert!(facts.syntax_error);
        assert_eq!(facts.symbols, []);
    }

    #[test]
    fn a_renamed_copy_has_the_same_tokens() {
        let kinds = |text: &str| {
            facts_of("src/a.py", text)
                .tokens
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>()
        };
        let original = kinds("def a(b):\n    return b + 1\n");
        assert_eq!(original, kinds("def q(r):\n    return r + 2\n"));
        assert_ne!(original, kinds("def a(b):\n    return b - 1\n"));
    }

    #[test]
    fn a_test_file_is_test_code_with_all_it_holds() {
        let facts = facts_of("tests/test_a.py", "def test_f():\n    pass\n");
        assert_eq!(facts.role, CodeRole::Test);
        assert_eq!(facts.test_lines, facts.lines);
        let f = facts.symbols.first().expect("a symbol");
        assert_eq!(f.kind, SymbolKind::Function);
        assert_eq!(f.role, CodeRole::Test);
    }
}
