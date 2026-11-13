//! What a file's path says about it.

use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::CodeRole;
use std::path::Path;

/// The language of a file, if it is Python: code, or a stub of types.
pub(crate) fn of(path: &RelativePath) -> Option<LanguageId> {
    let extension = Path::new(path.as_str()).extension()?;
    (extension == "py" || extension == "pyi").then_some(LanguageId::Python)
}

/// Test code as pytest finds it: `test_*.py`, `*_test.py`, `conftest.py`, and the
/// `tests` folder.
pub(crate) fn role(path: &RelativePath) -> CodeRole {
    let mut parts = path.as_str().split('/');
    let file = parts.next_back().unwrap_or_default();
    let stem = file
        .strip_suffix(".py")
        .or_else(|| file.strip_suffix(".pyi"))
        .unwrap_or(file);
    let test_file = stem.starts_with("test_") || stem.ends_with("_test") || stem == "conftest";
    if test_file || parts.any(|folder| folder == "tests") {
        CodeRole::Test
    } else {
        CodeRole::Production
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_and_stubs_are_ours() {
        let of_path = |path| of(&RelativePath::new(path));
        assert_eq!(of_path("src/a.py"), Some(LanguageId::Python));
        assert_eq!(of_path("src/a.pyi"), Some(LanguageId::Python));
        assert_eq!(of_path("src/a.pyc"), None);
    }

    #[test]
    fn test_code_is_found_as_pytest_finds_it() {
        let role_of = |path| role(&RelativePath::new(path));
        for path in [
            "src/test_a.py",
            "src/a_test.py",
            "conftest.py",
            "tests/a.py",
            "pkg/tests/unit/a.py",
        ] {
            assert_eq!(role_of(path), CodeRole::Test, "{path}");
        }
        for path in ["src/latest.py", "src/testing.py", "src/contest.py"] {
            assert_eq!(role_of(path), CodeRole::Production, "{path}");
        }
    }
}
