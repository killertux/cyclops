//! Sorts the lines of a Rust file, using the tokens the parser found.

use cyclops_domain::lines::LineMap;
use ra_ap_syntax::{NodeOrToken, SyntaxKind, SyntaxNode};

pub(crate) fn classify(text: &str, root: &SyntaxNode) -> LineMap {
    let mut map = LineMap::new(text);
    let tokens = root
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token);
    for token in tokens {
        let range = token.text_range();
        let (start, end) = (range.start().into(), range.end().into());
        match token.kind() {
            SyntaxKind::WHITESPACE => {}
            SyntaxKind::COMMENT | SyntaxKind::OUTER_DOC_COMMENT | SyntaxKind::INNER_DOC_COMMENT => {
                map.mark_comment(start, end);
            }
            _ => map.mark_code(start, end),
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyclops_domain::facts::LineCounts;
    use ra_ap_syntax::{AstNode, Edition, SourceFile};

    fn counts(text: &str) -> LineCounts {
        let tree = SourceFile::parse(text, Edition::Edition2024).tree();
        classify(text, tree.syntax()).counts()
    }

    fn lines(code: u32, comment: u32, blank: u32) -> LineCounts {
        LineCounts {
            code,
            comment,
            blank,
        }
    }

    #[test]
    fn a_multi_line_string_is_all_code() {
        assert_eq!(counts("const S: &str = \"a\n\n// b\";\n"), lines(3, 0, 0));
    }

    #[test]
    fn a_blank_line_inside_a_block_comment_is_a_comment() {
        assert_eq!(counts("/* a\n\n b */\n"), lines(0, 3, 0));
    }

    #[test]
    fn doc_comments_are_comments() {
        assert_eq!(counts("//! a\n/// b\nfn f() {}\n"), lines(1, 2, 0));
    }

    #[test]
    fn windows_line_breaks_count_the_same() {
        assert_eq!(counts("// a\r\n\r\nfn f() {}\r\n"), lines(1, 1, 1));
    }
}
