//! Duplication: copies of code, found by matching windows of tokens with names and
//! values made alike. See `specs/metrics/duplication.md`.

use crate::analysis::AnalysisInput;
use cyclops_domain::facts::{FileFacts, Token};
use cyclops_domain::metric::{Measurement, MetricId, Subject};
use cyclops_domain::model::TestCode;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::{Symbol, SymbolKind};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// How many other places a window is compared with, so that very common code cannot make
/// the work grow without limit.
const MAX_PARTNERS: usize = 64;

/// The copies a project has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Duplication {
    /// Each group of files that share a copy, in order, with its files in order.
    pub groups: Vec<Vec<RelativePath>>,
    /// For each file, where its copies are in other files, as ranges of lines.
    pub copies: BTreeMap<RelativePath, BTreeMap<RelativePath, Vec<(u32, u32)>>>,
    /// The lines of each file that are part of a copy.
    pub lines: BTreeMap<RelativePath, BTreeSet<u32>>,
}

/// The tokens of a file that count.
struct Stream<'a> {
    path: &'a RelativePath,
    family: u8,
    tokens: Vec<Token>,
}

/// A hash of a window of token kinds, one per start, computed as it rolls along.
fn window_hashes(tokens: &[Token], size: usize) -> Vec<u64> {
    const BASE: u64 = 1_000_003;
    if size == 0 || tokens.len() < size {
        return Vec::new();
    }
    let top = (1..size).fold(1_u64, |power, _| power.wrapping_mul(BASE));
    let mut hash = 0_u64;
    let mut hashes = Vec::with_capacity(tokens.len() - size + 1);
    for (index, token) in tokens.iter().enumerate() {
        if index >= size
            && let Some(leaving) = tokens.get(index - size)
        {
            hash = hash.wrapping_sub(u64::from(leaving.kind).wrapping_add(1).wrapping_mul(top));
        }
        hash = hash
            .wrapping_mul(BASE)
            .wrapping_add(u64::from(token.kind).wrapping_add(1));
        if index + 1 >= size {
            hashes.push(hash);
        }
    }
    hashes
}

/// For each start, whether the window there only repeats a shorter piece of itself, as
/// a list of `mod a;` lines does. Such a window matches any other list of its kind, and
/// that is not a copy.
fn repeating_windows(tokens: &[Token], size: usize) -> Vec<bool> {
    let starts = (tokens.len() + 1).saturating_sub(size);
    let mut repeating = vec![false; starts];
    for period in 1..=size / 2 {
        // How far from each place the code goes on matching itself one period later.
        let mut run = 0_usize;
        let mut runs = vec![0_usize; tokens.len()];
        for index in (0..tokens.len()).rev() {
            let same = tokens
                .get(index + period)
                .zip(tokens.get(index))
                .is_some_and(|(later, here)| later.kind == here.kind);
            run = if same { run + 1 } else { 0 };
            if let Some(slot) = runs.get_mut(index) {
                *slot = run;
            }
        }
        for (start, flag) in repeating.iter_mut().enumerate() {
            if runs.get(start).is_some_and(|run| *run >= size - period) {
                *flag = true;
            }
        }
    }
    repeating
}

/// Where each window of tokens starts, by language family and hash, leaving out the
/// windows that only repeat themselves.
fn window_places(streams: &[Stream<'_>], size: usize) -> HashMap<(u8, u64), Vec<(usize, usize)>> {
    let mut places: HashMap<(u8, u64), Vec<(usize, usize)>> = HashMap::new();
    for (index, stream) in streams.iter().enumerate() {
        let repeating = repeating_windows(&stream.tokens, size);
        for (start, hash) in window_hashes(&stream.tokens, size).into_iter().enumerate() {
            if repeating.get(start).copied().unwrap_or(false) {
                continue;
            }
            places
                .entry((stream.family, hash))
                .or_default()
                .push((index, start));
        }
    }
    places
}

fn find(group: &mut [usize], start: usize) -> usize {
    let mut root = start;
    while let Some(next) = group.get(root).copied().filter(|next| *next != root) {
        root = next;
    }
    root
}

/// The places of one window that are real copies. Code that repeats itself, such as a
/// long list of `mod a;` lines, matches itself a little further along, closer than the
/// length of a window, and that is not a copy.
fn real_copies(found: &[(usize, usize)], size: usize) -> Vec<(usize, usize)> {
    let mut sorted = found.to_vec();
    sorted.sort_unstable();
    sorted
        .iter()
        .enumerate()
        .filter(|(index, (file, start))| {
            let near = |other: Option<&(usize, usize)>| {
                other.is_some_and(|(other_file, other_start)| {
                    other_file == file && other_start.abs_diff(*start) < size
                })
            };
            let before = index
                .checked_sub(1)
                .and_then(|previous| sorted.get(previous));
            !near(before) && !near(sorted.get(index + 1))
        })
        .map(|(_, place)| *place)
        .collect()
}

/// Merges ranges of lines that overlap or touch.
fn merged(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.sort_unstable();
    let mut result: Vec<(u32, u32)> = Vec::new();
    for (start, end) in ranges {
        match result.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => result.push((start, end)),
        }
    }
    result
}

impl Duplication {
    /// Finds the copies among the files that count. Pieces shorter than `min_tokens` are
    /// left out.
    #[must_use]
    pub fn find(input: &AnalysisInput<'_>, facts: &[&FileFacts], min_tokens: u32) -> Self {
        let counted: BTreeSet<&RelativePath> =
            input.counted_files().map(|file| &file.path).collect();
        let streams: Vec<Stream<'_>> = facts
            .iter()
            .filter(|file| counted.contains(&file.path))
            .map(|file| Stream {
                path: &file.path,
                family: file.language.family(),
                tokens: file
                    .tokens
                    .iter()
                    .copied()
                    .filter(|token| input.tests == TestCode::Included || !token.test)
                    .collect(),
            })
            .collect();
        let size = usize::try_from(min_tokens).unwrap_or(usize::MAX).max(1);

        let places = window_places(&streams, size);

        let mut group: Vec<usize> = (0..streams.len()).collect();
        let mut lines: BTreeMap<RelativePath, BTreeSet<u32>> = BTreeMap::new();
        let mut copies: BTreeMap<RelativePath, BTreeMap<RelativePath, Vec<(u32, u32)>>> =
            BTreeMap::new();
        let span = |file: usize, start: usize| {
            let tokens = &streams.get(file)?.tokens;
            Some((tokens.get(start)?.line, tokens.get(start + size - 1)?.line))
        };
        for found in places.values().filter(|found| found.len() > 1) {
            let copies_found = real_copies(found, size);
            if copies_found.len() < 2 {
                continue;
            }
            let found = &copies_found;
            for &(file, start) in found {
                let Some((first, last)) = span(file, start) else {
                    continue;
                };
                let Some(stream) = streams.get(file) else {
                    continue;
                };
                lines
                    .entry(stream.path.clone())
                    .or_default()
                    .extend(first..=last);
                for &(other, other_start) in found.iter().take(MAX_PARTNERS) {
                    if other == file {
                        continue;
                    }
                    let (root, other_root) = (find(&mut group, file), find(&mut group, other));
                    if let Some(slot) = group.get_mut(root) {
                        *slot = other_root;
                    }
                    if let (Some(range), Some(partner)) =
                        (span(other, other_start), streams.get(other))
                    {
                        copies
                            .entry(stream.path.clone())
                            .or_default()
                            .entry(partner.path.clone())
                            .or_default()
                            .push(range);
                    }
                }
            }
        }
        for partners in copies.values_mut() {
            for ranges in partners.values_mut() {
                *ranges = merged(std::mem::take(ranges));
            }
        }

        let mut by_root: BTreeMap<usize, Vec<RelativePath>> = BTreeMap::new();
        for (index, stream) in streams.iter().enumerate() {
            if copies.contains_key(stream.path) {
                by_root
                    .entry(find(&mut group, index))
                    .or_default()
                    .push(stream.path.clone());
            }
        }
        let mut groups: Vec<Vec<RelativePath>> = by_root
            .into_values()
            .map(|mut files| {
                files.sort();
                files
            })
            .collect();
        groups.sort();
        Self {
            groups,
            copies,
            lines,
        }
    }

    /// The share of the code lines of each file, and of each module and package, that
    /// are part of a copy.
    #[must_use]
    pub fn measure(&self, input: &AnalysisInput<'_>) -> Vec<Measurement> {
        let mut totals: BTreeMap<Subject, (u32, u32)> = BTreeMap::new();
        for file in input.counted_files() {
            let code = file.counted_lines(input.tests).code;
            let copied = self
                .lines
                .get(&file.path)
                .map_or(0, |lines| u32::try_from(lines.len()).unwrap_or(u32::MAX));
            let mut subjects = vec![Subject::File(file.path.clone())];
            let mut current = file.module.clone();
            while let Some(id) = current.take() {
                let Some(symbol) = input.model.symbol(&id) else {
                    break;
                };
                if matches!(symbol.kind, SymbolKind::Module | SymbolKind::Package) {
                    subjects.push(Subject::Symbol(id.clone()));
                }
                current.clone_from(&symbol.parent);
            }
            for subject in subjects {
                let total = totals.entry(subject).or_insert((0, 0));
                total.0 += copied.min(code);
                total.1 += code;
            }
        }
        let counted: BTreeSet<&RelativePath> =
            input.counted_files().map(|file| &file.path).collect();
        for symbol in input.model.symbols() {
            if symbol.code_lines == 0 {
                continue;
            }
            let copied = match symbol.kind {
                SymbolKind::Function | SymbolKind::Method => self.copied_in(symbol, &counted),
                SymbolKind::Struct | SymbolKind::Class | SymbolKind::Enum => {
                    // A type is measured over all its lines, its methods' as well as its
                    // own, wherever they are written.
                    input
                        .model
                        .children(&symbol.id)
                        .filter(|child| child.kind == SymbolKind::Method)
                        .fold(self.copied_in(symbol, &counted), |sum, child| {
                            match (sum, self.copied_in(child, &counted)) {
                                (Some(sum), Some(more)) => Some(sum + more),
                                (sum, more) => sum.or(more),
                            }
                        })
                }
                _ => None,
            };
            if let Some(copied) = copied {
                totals.insert(
                    Subject::Symbol(symbol.id.clone()),
                    (copied.min(symbol.code_lines), symbol.code_lines),
                );
            }
        }
        totals
            .into_iter()
            .filter(|(_, (_, code))| *code > 0)
            .map(|(subject, (copied, code))| Measurement {
                metric: MetricId::Duplication,
                subject,
                value: f64::from(copied) * 100.0 / f64::from(code),
            })
            .collect()
    }

    /// How many lines of a symbol's own place are part of a copy, or nothing when its
    /// file is not counted.
    fn copied_in(&self, symbol: &Symbol, counted: &BTreeSet<&RelativePath>) -> Option<u32> {
        let location = symbol.location.as_ref()?;
        if !counted.contains(&location.file) {
            return None;
        }
        let span = location.span;
        let copied = self.lines.get(&location.file).map_or(0, |lines| {
            lines
                .range(span.start_line..=span.end_line.max(span.start_line))
                .count()
        });
        Some(u32::try_from(copied).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(kinds: &[u16]) -> Vec<Token> {
        (0_u32..)
            .zip(kinds)
            .map(|(line, kind)| Token {
                kind: *kind,
                line: line + 1,
                test: false,
            })
            .collect()
    }

    #[test]
    fn equal_windows_hash_alike_wherever_they_are() {
        let hashes = window_hashes(&tokens(&[5, 6, 7, 9, 5, 6, 7]), 3);
        assert_eq!(hashes.len(), 5);
        assert_eq!(hashes.first(), hashes.get(4));
        assert_ne!(hashes.first(), hashes.get(1));
    }

    #[test]
    fn too_few_tokens_have_no_window() {
        assert_eq!(window_hashes(&tokens(&[1, 2]), 3), Vec::<u64>::new());
    }

    fn stream_facts(path: &str, kinds: &[u16]) -> FileFacts {
        FileFacts {
            path: RelativePath::new(path),
            language: cyclops_domain::language::LanguageId::Rust,
            role: cyclops_domain::symbol::CodeRole::Production,
            lines: cyclops_domain::facts::LineCounts {
                code: u32::try_from(kinds.len()).unwrap_or(0),
                comment: 0,
                blank: 0,
            },
            test_lines: cyclops_domain::facts::LineCounts::default(),
            symbols: Vec::new(),
            imports: Vec::new(),
            syntax_error: false,
            test_modules: Vec::new(),
            extensions: Vec::new(),
            exports: Vec::new(),
            tokens: tokens(kinds),
        }
    }

    fn found(files: &[FileFacts], size: u32) -> Duplication {
        let analyzed: Vec<crate::analysis::AnalyzedFile> = files
            .iter()
            .map(crate::analysis::AnalyzedFile::from_facts)
            .collect();
        let model = cyclops_domain::model::ProjectModel::default();
        let input = AnalysisInput {
            model: cyclops_domain::model::ModelView::new(&model, TestCode::Excluded),
            files: &analyzed,
            tests: TestCode::Excluded,
        };
        let facts: Vec<&FileFacts> = files.iter().collect();
        Duplication::find(&input, &facts, size)
    }

    #[test]
    fn code_that_only_repeats_itself_is_not_a_copy() {
        let pattern = [2, 3, 0, 4];
        let list = stream_facts("lib.rs", &pattern.repeat(6));
        let other = stream_facts("mod.rs", &pattern.repeat(6));
        assert_eq!(found(&[list, other], 8).lines, BTreeMap::new());
    }

    #[test]
    fn a_short_list_that_repeats_itself_is_not_a_copy() {
        // Too short for every window to have a neighbor in its own file.
        let pattern = [2, 3, 0, 4];
        let list = stream_facts("lib.rs", &pattern.repeat(3));
        let other = stream_facts("mod.rs", &pattern.repeat(3));
        assert_eq!(found(&[list, other], 8).lines, BTreeMap::new());
    }

    #[test]
    fn windows_that_repeat_a_shorter_piece_are_found() {
        let repeating = repeating_windows(&tokens(&[1, 2, 1, 2, 1, 2, 3]), 4);
        assert_eq!(repeating, [true, true, true, false]);
    }

    #[test]
    fn the_same_code_twice_in_one_file_is_a_copy() {
        let twice = stream_facts("a.rs", &[5, 6, 7, 8, 9, 9, 5, 6, 7, 8]);
        let lines = found(&[twice], 4).lines;
        let copied: Vec<u32> = lines.values().flatten().copied().collect();
        assert_eq!(copied, [1, 2, 3, 4, 7, 8, 9, 10]);
    }

    #[test]
    fn a_copy_in_another_file_is_a_group() {
        let a = stream_facts("a.rs", &[5, 6, 7, 8, 1]);
        let b = stream_facts("b.rs", &[2, 5, 6, 7, 8]);
        let duplication = found(&[a, b], 4);
        let group: Vec<&str> = duplication
            .groups
            .iter()
            .flatten()
            .map(RelativePath::as_str)
            .collect();
        assert_eq!(group, ["a.rs", "b.rs"]);
        let copies = duplication.copies.get(&RelativePath::new("a.rs"));
        let ranges = copies.and_then(|partners| partners.get(&RelativePath::new("b.rs")));
        assert_eq!(ranges, Some(&vec![(2, 5)]));
    }

    #[test]
    fn ranges_that_touch_are_merged() {
        assert_eq!(
            merged(vec![(5, 7), (1, 3), (4, 4), (10, 12)]),
            [(1, 7), (10, 12)]
        );
    }
}
