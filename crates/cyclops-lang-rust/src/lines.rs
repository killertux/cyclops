//! Sorts the lines of a file into code, comments and blanks.

use cyclops_domain::facts::LineCounts;
use ra_ap_syntax::{NodeOrToken, SyntaxKind, SyntaxNode, TextSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Blank,
    Comment,
    Code,
}

pub(crate) struct LineMap {
    /// Where each line starts.
    starts: Vec<TextSize>,
    kinds: Vec<LineKind>,
}

impl LineMap {
    pub(crate) fn new(text: &str, root: &SyntaxNode) -> Self {
        let mut map = Self::blank(text);
        map.mark_tokens(root);
        map
    }

    /// The lines of a file that was not parsed, which cannot tell comments from code.
    pub(crate) fn unparsed(text: &str) -> Self {
        let mut map = Self::blank(text);
        for (line, content) in (1..).zip(text.lines()) {
            if !content.trim().is_empty() {
                map.mark(line, LineKind::Code);
            }
        }
        map
    }

    fn blank(text: &str) -> Self {
        let mut starts = Vec::new();
        let mut at_line_start = true;
        for (offset, character) in text.char_indices() {
            if at_line_start {
                starts.push(TextSize::new(u32::try_from(offset).unwrap_or(u32::MAX)));
            }
            at_line_start = character == '\n';
        }
        let kinds = vec![LineKind::Blank; starts.len()];
        Self { starts, kinds }
    }

    fn mark_tokens(&mut self, root: &SyntaxNode) {
        let tokens = root
            .descendants_with_tokens()
            .filter_map(NodeOrToken::into_token);
        for token in tokens {
            let kind = match token.kind() {
                SyntaxKind::WHITESPACE => continue,
                SyntaxKind::COMMENT
                | SyntaxKind::OUTER_DOC_COMMENT
                | SyntaxKind::INNER_DOC_COMMENT => LineKind::Comment,
                _ => LineKind::Code,
            };
            let range = token.text_range();
            let last = range.end().checked_sub(TextSize::new(1));
            let lines = self.line_of(range.start())..=self.line_of(last.unwrap_or(range.end()));
            for line in lines {
                self.mark(line, kind);
            }
        }
    }

    /// A line with both code and a comment is code.
    fn mark(&mut self, line: u32, kind: LineKind) {
        let index = usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX);
        if let Some(existing) = self.kinds.get_mut(index)
            && (*existing == LineKind::Blank || kind == LineKind::Code)
        {
            *existing = kind;
        }
    }

    /// The line an offset is on, counted from 1.
    pub(crate) fn line_of(&self, offset: TextSize) -> u32 {
        let line = self.starts.partition_point(|start| *start <= offset);
        u32::try_from(line).unwrap_or(u32::MAX).max(1)
    }

    pub(crate) fn counts(&self) -> LineCounts {
        Self::tally(self.kinds.iter())
    }

    /// The counts of a range of lines, counted from 1, both ends included.
    pub(crate) fn counts_between(&self, start: u32, end: u32) -> LineCounts {
        let skip = usize::try_from(start.saturating_sub(1)).unwrap_or(usize::MAX);
        let take = usize::try_from(end.saturating_sub(start) + 1).unwrap_or(usize::MAX);
        Self::tally(self.kinds.iter().skip(skip).take(take))
    }

    fn tally<'a>(kinds: impl Iterator<Item = &'a LineKind>) -> LineCounts {
        let mut counts = LineCounts::default();
        for kind in kinds {
            match kind {
                LineKind::Code => counts.code += 1,
                LineKind::Comment => counts.comment += 1,
                LineKind::Blank => counts.blank += 1,
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra_ap_syntax::{AstNode, Edition, SourceFile};

    fn counts(text: &str) -> LineCounts {
        let tree = SourceFile::parse(text, Edition::Edition2024).tree();
        LineMap::new(text, tree.syntax()).counts()
    }

    fn lines(code: u32, comment: u32, blank: u32) -> LineCounts {
        LineCounts {
            code,
            comment,
            blank,
        }
    }

    #[test]
    fn an_empty_file_has_no_lines() {
        assert_eq!(counts(""), lines(0, 0, 0));
    }

    #[test]
    fn a_final_line_break_adds_no_line() {
        assert_eq!(counts("fn f() {}\n"), lines(1, 0, 0));
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
