//! Extracts the facts of one Elixir file.

use crate::language;
use crate::symbols::{Extractor, attribute_name};
use crate::tree::{each_node, end, start};
use cyclops_domain::facts::{FileFacts, LineCounts, Token, is_too_deep};
use cyclops_domain::language::LanguageId;
use cyclops_domain::lines::LineMap;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::CodeRole;
use tree_sitter::{Node, Parser, Tree};

/// The attributes that document code, and so count as comments.
const DOCUMENTATION: [&str; 3] = ["doc", "moduledoc", "typedoc"];

pub(crate) fn extract(path: &RelativePath, text: &str) -> FileFacts {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let role = language::role(path);
    let tree = if is_too_deep(text) { None } else { parse(text) };
    let Some(tree) = tree else {
        return unparsed(path, text, role);
    };
    let root = tree.root_node();
    let comments = comments(root, text);
    let mut lines = LineMap::new(text);
    for (from, to) in &comments {
        lines.mark_comment(*from, *to);
    }
    lines.mark_code_outside(text, &comments);
    let mut extractor = Extractor::new(&lines, text, role);
    extractor.file(root);
    let (symbols, imports, extensions) = extractor.into_parts();
    let counts = lines.counts();
    FileFacts {
        path: path.clone(),
        language: LanguageId::Elixir,
        role,
        lines: counts,
        test_lines: test_lines(role, counts),
        symbols,
        imports,
        syntax_error: root.has_error(),
        test_modules: Vec::new(),
        extensions,
        exports: Vec::new(),
        tokens: tokens(root, &lines, role == CodeRole::Test, &comments),
    }
}

/// Parses a file. A grammar the library cannot load leaves every file unparsed, which
/// shows as a syntax error in each of them rather than as a crash.
fn parse(text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_elixir::LANGUAGE.into())
        .is_ok()
        .then(|| parser.parse(text, None))
        .flatten()
}

/// A file that could not be parsed at all still has lines.
fn unparsed(path: &RelativePath, text: &str, role: CodeRole) -> FileFacts {
    let counts = LineMap::unparsed(text).counts();
    FileFacts {
        path: path.clone(),
        language: LanguageId::Elixir,
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

/// Where each comment is, in order: `#` comments, and the attributes that document code,
/// as a Rust doc comment is one.
fn comments(root: Node<'_>, source: &str) -> Vec<(u32, u32)> {
    let mut found = Vec::new();
    each_node(root, |node| {
        let documentation =
            attribute_name(node, source).is_some_and(|name| DOCUMENTATION.contains(&name));
        if node.kind() == "comment" || documentation {
            found.push((start(node), end(node)));
        }
    });
    found.sort_unstable();
    // A comment inside documentation is part of it.
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(found.len());
    for (from, to) in found {
        match merged.last_mut() {
            Some((_, last)) if from < *last => *last = (*last).max(to),
            _ => merged.push((from, to)),
        }
    }
    merged
}

/// The code as tokens, with names and literal values made alike, for finding copies.
fn tokens(root: Node<'_>, lines: &LineMap, test: bool, comments: &[(u32, u32)]) -> Vec<Token> {
    let mut found = Vec::new();
    each_node(root, |node| {
        let at = start(node);
        let in_comment = comments.iter().any(|(from, to)| *from <= at && at < *to);
        if node.child_count() == 0 && !node.is_missing() && !in_comment {
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
        "identifier" | "alias" | "keyword" => Token::NAME,
        "integer" | "float" | "quoted_content" | "atom" | "char" | "boolean" | "nil" => {
            Token::LITERAL
        }
        _ => Token::FIRST_OTHER.saturating_add(node.kind_id()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts_of(path: &str, text: &str) -> FileFacts {
        extract(&RelativePath::new(path), text)
    }

    #[test]
    fn documentation_is_a_comment() {
        let facts = facts_of(
            "lib/a.ex",
            "# c\n\ndefmodule M do\n  @doc \"\"\"\n  a\n  \"\"\"\n  def f, do: 1\nend\n",
        );
        assert_eq!(
            facts.lines,
            LineCounts {
                code: 3,
                comment: 4,
                blank: 1,
            }
        );
    }

    #[test]
    fn a_comment_in_a_string_is_not_a_comment() {
        let facts = facts_of("lib/a.ex", "x = \"# b\"\n");
        assert_eq!(facts.lines.comment, 0);
        assert_eq!(facts.lines.code, 1);
    }

    #[test]
    fn what_comes_before_a_syntax_error_is_kept() {
        let facts = facts_of(
            "lib/bad.ex",
            "defmodule Bad do\n  def kept, do: 1\n\n  def broken(, do: 2\nend\n",
        );
        assert!(facts.syntax_error);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "kept/0"));
    }

    #[test]
    fn code_too_deep_for_the_parser_is_a_syntax_error_and_not_a_crash() {
        let depth = 20_000;
        let text = format!("x = {}1{}\n", "[".repeat(depth), "]".repeat(depth));
        let facts = facts_of("lib/a.ex", &text);
        assert!(facts.syntax_error);
        assert_eq!(facts.symbols, []);
    }

    #[test]
    fn a_renamed_copy_has_the_same_tokens() {
        let kinds = |text: &str| {
            facts_of("lib/a.ex", text)
                .tokens
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>()
        };
        let original = kinds("def a(b), do: b + 1");
        assert_eq!(original, kinds("def q(r), do: r + 2"));
        assert_ne!(original, kinds("def a(b), do: b - 1"));
    }

    #[test]
    fn a_test_file_is_test_code() {
        let facts = facts_of("test/a_test.exs", "defmodule ATest do\nend\n");
        assert_eq!(facts.role, CodeRole::Test);
        assert_eq!(facts.test_lines, facts.lines);
    }
}
