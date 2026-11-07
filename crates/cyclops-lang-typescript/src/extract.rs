//! Extracts the facts of one TypeScript or JavaScript file.

use crate::language;
use crate::symbols::Extractor;
use cyclops_domain::facts::{Export, FileFacts, Import, LineCounts, SymbolFact, is_too_deep};
use cyclops_domain::lines::LineMap;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::CodeRole;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// How many earlier places to try cutting a broken file at.
const MAX_CUTS: usize = 8;

/// What a parse gives, for the part of a file that parsed.
struct Parsed {
    symbols: Vec<SymbolFact>,
    imports: Vec<Import>,
    exports: Vec<Export>,
    comments: Vec<(u32, u32)>,
    syntax_error: bool,
}

pub(crate) fn extract(path: &RelativePath, text: &str) -> FileFacts {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if is_too_deep(text) {
        return facts(path, text, None);
    }
    let source_type = language::source_type(path);
    let role = language::role(path);
    let parsed = parse(text, text, source_type, role).or_else(|| salvage(text, source_type, role));
    facts(path, text, parsed)
}

/// Parses `part`, the start of `text`. A fatal error gives nothing at all, because the
/// parser does not recover from one.
fn parse(text: &str, part: &str, source_type: SourceType, role: CodeRole) -> Option<Parsed> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, part, source_type).parse();
    if parsed.fatal_error {
        return None;
    }
    let map = LineMap::new(text);
    let mut comments: Vec<(u32, u32)> = parsed
        .program
        .comments
        .iter()
        .map(|comment| (comment.span.start, comment.span.end))
        .collect();
    comments.sort_unstable();
    let lines = classified(map, text, &comments);
    let mut extractor = Extractor::new(&lines, part, role);
    extractor.program(&parsed.program);
    let (symbols, imports, exports) = extractor.into_parts();
    Some(Parsed {
        symbols,
        imports,
        exports,
        comments,
        syntax_error: parsed.diagnostics.has_errors(),
    })
}

fn classified(mut map: LineMap, text: &str, comments: &[(u32, u32)]) -> LineMap {
    for (start, end) in comments {
        map.mark_comment(*start, *end);
    }
    map.mark_code_outside(text, comments);
    map
}

/// Code around a syntax error still counts, so this keeps what comes before it. It cuts
/// the file where a top level statement could start, latest first, until it parses.
fn salvage(text: &str, source_type: SourceType, role: CodeRole) -> Option<Parsed> {
    top_level_lines(text)
        .into_iter()
        .rev()
        .take(MAX_CUTS)
        .find_map(|cut| {
            let part = text.get(..cut)?;
            parse(text, part, source_type, role)
        })
        .map(|parsed| Parsed {
            syntax_error: true,
            ..parsed
        })
}

/// The lines that start outside any bracket. This reads brackets in strings too, so it
/// is only a guess, and parsing is what proves a cut good.
fn top_level_lines(text: &str) -> Vec<usize> {
    let mut depth = 0_usize;
    let mut cuts = Vec::new();
    for (offset, byte) in text.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'\n' if depth == 0 => cuts.push(offset + 1),
            _ => {}
        }
    }
    cuts
}

fn facts(path: &RelativePath, text: &str, parsed: Option<Parsed>) -> FileFacts {
    let role = language::role(path);
    let (symbols, imports, exports, comments, syntax_error) = parsed.map_or_else(
        || (Vec::new(), Vec::new(), Vec::new(), Vec::new(), true),
        |parsed| {
            (
                parsed.symbols,
                parsed.imports,
                parsed.exports,
                parsed.comments,
                parsed.syntax_error,
            )
        },
    );
    let lines = classified(LineMap::new(text), text, &comments).counts();
    FileFacts {
        path: path.clone(),
        language: language::of(path).unwrap_or(cyclops_domain::language::LanguageId::TypeScript),
        role,
        lines,
        test_lines: match role {
            CodeRole::Test => lines,
            CodeRole::Production => LineCounts::default(),
        },
        symbols,
        imports,
        syntax_error,
        test_modules: Vec::new(),
        extensions: Vec::new(),
        exports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyclops_domain::language::LanguageId;
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
        let text = "export function kept() {\n  return 1;\n}\n\nexport function broken( {\n";
        let facts = facts_of("a.ts", text);
        assert!(facts.syntax_error);
        assert_eq!(names(&facts), ["kept"]);
    }

    #[test]
    fn a_syntax_error_in_the_middle_keeps_what_comes_before_it() {
        let facts = facts_of(
            "a.ts",
            "function a() {}\nfunction b( { let x = ;\nfunction c() {}\n",
        );
        assert!(facts.syntax_error);
        assert_eq!(names(&facts), ["a"]);
    }

    #[test]
    fn the_lines_of_a_broken_file_all_count() {
        let facts = facts_of("a.ts", "// c\nfunction a() {}\n\nfunction b( {\n");
        assert_eq!(
            facts.lines,
            LineCounts {
                code: 2,
                comment: 1,
                blank: 1
            }
        );
    }

    #[test]
    fn a_file_with_nothing_to_salvage_is_still_reported() {
        let facts = facts_of("a.ts", "function b( {");
        assert!(facts.syntax_error);
        assert_eq!(facts.symbols, []);
        assert_eq!(facts.lines.code, 1);
    }

    #[test]
    fn code_too_deep_for_the_parser_is_a_syntax_error_and_not_a_crash() {
        let depth = 20_000;
        let text = format!("const a = {}1{};\n", "(".repeat(depth), ")".repeat(depth));
        let facts = facts_of("a.ts", &text);
        assert!(facts.syntax_error);
        assert_eq!(facts.symbols, []);
    }

    #[test]
    fn each_extension_is_parsed_the_way_it_needs() {
        let cases = [
            ("a.tsx", "export const A = () => <p>hello</p>;\n"),
            ("a.jsx", "export const A = () => <p>hello</p>;\n"),
            ("a.js", "export const A = () => <p>hello</p>;\n"),
            ("a.d.ts", "export declare function f(): void;\n"),
            ("a.ts", "const f = <T,>(a: T) => a;\n"),
            ("a.cjs", "module.exports = {};\n"),
            ("a.js", "const fs = require('fs');\nmodule.exports = fs;\n"),
        ];
        for (path, text) in cases {
            assert!(!facts_of(path, text).syntax_error, "{path}: {text}");
        }
    }

    #[test]
    fn a_comment_in_a_string_is_not_a_comment() {
        let facts = facts_of("a.ts", "const a = '// b';\n");
        assert_eq!(facts.lines.comment, 0);
        assert_eq!(facts.lines.code, 1);
    }

    #[test]
    fn a_file_knows_its_language() {
        assert_eq!(facts_of("a.mts", "").language, LanguageId::TypeScript);
        assert_eq!(facts_of("a.cjs", "").language, LanguageId::JavaScript);
    }

    #[test]
    fn a_test_file_is_test_code_with_all_it_holds() {
        let facts = facts_of("a.test.ts", "function f() {}\n");
        assert_eq!(facts.role, CodeRole::Test);
        assert_eq!(facts.test_lines, facts.lines);
        let f = facts.symbols.first().expect("a symbol");
        assert_eq!(f.role, CodeRole::Test);
        assert_eq!(f.kind, SymbolKind::Function);
    }
}
