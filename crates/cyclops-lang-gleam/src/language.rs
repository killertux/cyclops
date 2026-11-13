//! What a file's path says about it.

use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::CodeRole;
use std::path::Path;

/// The language of a file, if it is Gleam.
pub(crate) fn of(path: &RelativePath) -> Option<LanguageId> {
    let extension = Path::new(path.as_str()).extension()?;
    (extension == "gleam").then_some(LanguageId::Gleam)
}

/// Gleam keeps its tests in a `test` folder.
pub(crate) fn role(path: &RelativePath) -> CodeRole {
    let mut parts = path.as_str().split('/');
    parts.next_back();
    if parts.any(|folder| folder == "test") {
        CodeRole::Test
    } else {
        CodeRole::Production
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_gleam_files_are_ours() {
        assert_eq!(
            of(&RelativePath::new("src/a.gleam")),
            Some(LanguageId::Gleam)
        );
        assert_eq!(of(&RelativePath::new("src/a.rs")), None);
        assert_eq!(of(&RelativePath::new("gleam.toml")), None);
    }

    #[test]
    fn the_test_folder_holds_test_code() {
        let role_of = |path| role(&RelativePath::new(path));
        assert_eq!(role_of("test/a_test.gleam"), CodeRole::Test);
        assert_eq!(role_of("packages/x/test/a.gleam"), CodeRole::Test);
        assert_eq!(role_of("src/test.gleam"), CodeRole::Production);
        assert_eq!(role_of("src/testing/a.gleam"), CodeRole::Production);
    }
}
