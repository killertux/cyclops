//! What the `Cli` fixture needs: the real `cyclops` binary, and ways to read what it did.

use crate::read::yes_no;
use crate::scenario::Scenario;
use crate::scratch::repository;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

/// Builds the binary once, the first time a row needs it.
fn binary() -> Result<&'static Path, String> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let built = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
                .args(["build", "--quiet", "-p", "cyclops-cli", "--bin", "cyclops"])
                .current_dir(repository())
                .output()
                .map_err(|e| e.to_string())?;
            if !built.status.success() {
                return Err(String::from_utf8_lossy(&built.stderr).into_owned());
            }
            let fixtures = env::current_exe().map_err(|e| e.to_string())?;
            let folder = fixtures.parent().ok_or("the fixtures have no folder")?;
            Ok(folder.join("cyclops"))
        })
        .as_deref()
        .map_err(Clone::clone)
}

/// Splits a command the way a shell would, keeping quoted parts together.
fn words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut started = false;
    for character in command.chars() {
        let inside = quote.is_some();
        if quote == Some(character) {
            quote = None;
        } else if !inside && matches!(character, '\'' | '"') {
            quote = Some(character);
            started = true;
        } else if !inside && character.is_whitespace() {
            if started || !current.is_empty() {
                words.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(character);
        }
    }
    if started || !current.is_empty() {
        words.push(current);
    }
    words
}

pub fn run(scenario: &Scenario, arguments: &str) -> Result<Output, String> {
    let mut command = Command::new(binary()?);
    command
        .args(words(arguments))
        .current_dir(scenario.root())
        .env_clear()
        .envs(scenario.variables())
        .env("PATH", env::var("PATH").unwrap_or_default());
    command.output().map_err(|e| e.to_string())
}

/// Every file in a folder, hidden ones included.
pub fn files_in(root: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if entry.file_name() != ".git" {
                    pending.push(path);
                }
            } else if let Ok(inside) = path.strip_prefix(root) {
                found.insert(inside.display().to_string());
            }
        }
    }
    found
}

/// Whether a text is in some output. `nothing` asks whether there is no output at all.
pub fn includes(output: &str, text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    if text == "nothing" {
        return yes_no(output.trim().is_empty());
    }
    yes_no(output.contains(text))
}
