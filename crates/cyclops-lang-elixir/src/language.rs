//! What a file's path says about it.

use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::CodeRole;
use std::path::Path;

/// The language of a file, if it is Elixir: code, or a script.
pub(crate) fn of(path: &RelativePath) -> Option<LanguageId> {
    let extension = Path::new(path.as_str()).extension()?;
    (extension == "ex" || extension == "exs").then_some(LanguageId::Elixir)
}

/// Mix keeps tests in a `test` folder.
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
    fn code_and_scripts_are_ours() {
        let of_path = |path| of(&RelativePath::new(path));
        assert_eq!(of_path("lib/a.ex"), Some(LanguageId::Elixir));
        assert_eq!(of_path("mix.exs"), Some(LanguageId::Elixir));
        assert_eq!(of_path("lib/a.rs"), None);
        assert_eq!(of_path("lib/a.eex"), None);
    }

    #[test]
    fn the_test_folder_holds_test_code() {
        let role_of = |path| role(&RelativePath::new(path));
        assert_eq!(role_of("test/a_test.exs"), CodeRole::Test);
        assert_eq!(role_of("apps/x/test/a.exs"), CodeRole::Test);
        assert_eq!(role_of("lib/test.ex"), CodeRole::Production);
    }
}
