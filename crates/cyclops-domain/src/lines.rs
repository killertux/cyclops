//! Sorts the lines of a file into code, comments and blanks.

use crate::facts::LineCounts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Blank,
    Comment,
    Code,
}

/// The lines of one file. Offsets are in bytes, and lines are counted from 1.
#[derive(Debug, Clone)]
pub struct LineMap {
    /// Where each line starts.
    starts: Vec<u32>,
    kinds: Vec<LineKind>,
}

fn to_offset(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
}

fn to_index(line: u32) -> usize {
    usize::try_from(line).unwrap_or(usize::MAX)
}

impl LineMap {
    /// Every line starts out blank, until something marks it.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut starts = Vec::new();
        let mut at_line_start = true;
        for (offset, character) in text.char_indices() {
            if at_line_start {
                starts.push(to_offset(offset));
            }
            at_line_start = character == '\n';
        }
        let kinds = vec![LineKind::Blank; starts.len()];
        Self { starts, kinds }
    }

    /// The lines of a file that was not parsed, which cannot tell comments from code.
    #[must_use]
    pub fn unparsed(text: &str) -> Self {
        let mut map = Self::new(text);
        map.mark_code_outside(text, &[]);
        map
    }

    /// Marks the lines from `start` up to, but not including, `end` as code.
    pub fn mark_code(&mut self, start: u32, end: u32) {
        self.mark(start, end, LineKind::Code);
    }

    /// Marks the lines from `start` up to, but not including, `end` as a comment.
    pub fn mark_comment(&mut self, start: u32, end: u32) {
        self.mark(start, end, LineKind::Comment);
    }

    /// Marks as code every line with text outside the comments, given as sorted ranges.
    pub fn mark_code_outside(&mut self, text: &str, comments: &[(u32, u32)]) {
        let mut cursor = 0;
        for (start, end) in comments {
            self.mark_text(text, cursor, *start);
            cursor = *end;
        }
        self.mark_text(text, cursor, to_offset(text.len()));
    }

    fn mark_text(&mut self, text: &str, start: u32, end: u32) {
        let Some(part) = text.get(to_index(start)..to_index(end)) else {
            return;
        };
        let mut offset = start;
        for line in part.split_inclusive('\n') {
            let indent = line.len() - line.trim_start().len();
            if !line.trim().is_empty() {
                let at = offset.saturating_add(to_offset(indent));
                self.mark(at, at.saturating_add(1), LineKind::Code);
            }
            offset = offset.saturating_add(to_offset(line.len()));
        }
    }

    /// A line with both code and a comment is code.
    fn mark(&mut self, start: u32, end: u32, kind: LineKind) {
        let last = end.saturating_sub(1).max(start);
        for line in self.line_of(start)..=self.line_of(last) {
            let index = to_index(line.saturating_sub(1));
            if let Some(existing) = self.kinds.get_mut(index)
                && (*existing == LineKind::Blank || kind == LineKind::Code)
            {
                *existing = kind;
            }
        }
    }

    /// The line an offset is on.
    #[must_use]
    pub fn line_of(&self, offset: u32) -> u32 {
        let line = self.starts.partition_point(|start| *start <= offset);
        to_offset(line).max(1)
    }

    #[must_use]
    pub fn counts(&self) -> LineCounts {
        Self::tally(self.kinds.iter())
    }

    /// The counts of a range of lines, both ends included.
    #[must_use]
    pub fn counts_between(&self, start: u32, end: u32) -> LineCounts {
        let skip = to_index(start.saturating_sub(1));
        let take = to_index(end.saturating_sub(start)).saturating_add(1);
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

    fn lines(code: u32, comment: u32, blank: u32) -> LineCounts {
        LineCounts {
            code,
            comment,
            blank,
        }
    }

    #[test]
    fn an_empty_file_has_no_lines() {
        assert_eq!(LineMap::new("").counts(), lines(0, 0, 0));
    }

    #[test]
    fn a_final_line_break_adds_no_line() {
        assert_eq!(LineMap::unparsed("a\n").counts(), lines(1, 0, 0));
    }

    #[test]
    fn code_wins_on_a_line_with_both() {
        let text = "a // b\n";
        let mut map = LineMap::new(text);
        map.mark_comment(2, 6);
        map.mark_code_outside(text, &[(2, 6)]);
        assert_eq!(map.counts(), lines(1, 0, 0));
    }

    #[test]
    fn a_comment_can_span_lines_and_code_can_follow_it() {
        let text = "/* a\n\n b */ c\n/* d */\n";
        let comments = [(0, 11), (14, 21)];
        let mut map = LineMap::new(text);
        for (start, end) in comments {
            map.mark_comment(start, end);
        }
        map.mark_code_outside(text, &comments);
        assert_eq!(map.counts(), lines(1, 3, 0));
    }

    #[test]
    fn lines_can_be_counted_in_a_range() {
        let map = LineMap::unparsed("a\n\nb\n");
        assert_eq!(map.counts_between(2, 3), lines(1, 0, 1));
    }
}
