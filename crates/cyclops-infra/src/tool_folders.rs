//! Folders that hold what other tools installed or built, whatever `.gitignore` says.

use std::path::Path;

/// Skipped wherever they are.
const ALWAYS: [&str; 2] = ["node_modules", "target"];

/// Skipped only beside the manifest of the tool that fills them, since the names alone are
/// common: Gleam builds into `build`.
const BESIDE_MANIFEST: [(&str, &str); 1] = [("build", "gleam.toml")];

/// Whether a folder holds what a tool installed or built.
pub(crate) fn is_tool_folder(folder: &Path) -> bool {
    let Some(name) = folder.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    ALWAYS.contains(&name)
        || BESIDE_MANIFEST.iter().any(|(built, manifest)| {
            name == *built
                && folder
                    .parent()
                    .is_some_and(|parent| parent.join(manifest).is_file())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn some_folders_are_always_a_tool_s() {
        assert!(is_tool_folder(Path::new("web/node_modules")));
        assert!(is_tool_folder(Path::new("target")));
        assert!(!is_tool_folder(Path::new("src")));
    }

    #[test]
    fn a_build_folder_is_a_tool_s_only_beside_its_manifest() {
        let root = std::env::temp_dir().join(format!("cyclops-tools-{}", std::process::id()));
        fs::create_dir_all(root.join("gleam/build")).unwrap();
        fs::create_dir_all(root.join("other/build")).unwrap();
        fs::write(root.join("gleam/gleam.toml"), "name = \"shop\"\n").unwrap();
        assert!(is_tool_folder(&root.join("gleam/build")));
        assert!(!is_tool_folder(&root.join("other/build")));
        fs::remove_dir_all(&root).unwrap();
    }
}
