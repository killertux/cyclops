//! What a file's path says about it.

use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::CodeRole;
use oxc_span::SourceType;
use std::path::Path;

/// The language of a file, if it is one of this adapter's.
pub(crate) fn of(path: &RelativePath) -> Option<LanguageId> {
    let extension = Path::new(path.as_str()).extension()?.to_str()?;
    match extension {
        "ts" | "tsx" | "mts" | "cts" => Some(LanguageId::TypeScript),
        "js" | "jsx" | "mjs" | "cjs" => Some(LanguageId::JavaScript),
        _ => None,
    }
}

/// How to parse a file. A `.js` file may be a module or a script, and may hold JSX.
pub(crate) fn source_type(path: &RelativePath) -> SourceType {
    let parsed = SourceType::from_path(path.as_str()).unwrap_or_else(|_| SourceType::mjs());
    let extension = Path::new(path.as_str()).extension();
    if extension.is_some_and(|extension| extension == "js") {
        parsed.with_jsx(true).with_unambiguous(true)
    } else {
        parsed
    }
}

/// `*.test.*`, `*.spec.*`, and the usual folders hold test code.
pub(crate) fn role(path: &RelativePath) -> CodeRole {
    let mut parts: Vec<&str> = path.as_str().split('/').collect();
    let file_name = parts.pop().unwrap_or_default();
    let test_folder = parts
        .iter()
        .any(|folder| matches!(*folder, "__tests__" | "tests" | "test"));
    if test_folder || file_name.contains(".test.") || file_name.contains(".spec.") {
        CodeRole::Test
    } else {
        CodeRole::Production
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_is_recognized_by_its_path() {
        for path in [
            "src/a.test.ts",
            "src/a.spec.tsx",
            "src/__tests__/a.ts",
            "tests/a.ts",
            "packages/x/test/a.js",
        ] {
            assert_eq!(role(&RelativePath::new(path)), CodeRole::Test, "{path}");
        }
    }

    #[test]
    fn a_name_that_only_looks_like_test_code_is_not() {
        for path in ["src/latest.ts", "src/testing/a.ts", "src/contest.spec"] {
            assert_eq!(
                role(&RelativePath::new(path)),
                CodeRole::Production,
                "{path}"
            );
        }
    }

    #[test]
    fn other_files_are_not_ours() {
        assert_eq!(of(&RelativePath::new("src/a.rs")), None);
        assert_eq!(of(&RelativePath::new("Makefile")), None);
    }
}
