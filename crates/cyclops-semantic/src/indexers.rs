//! The indexers Cyclops knows, and how to run them.
//!
//! A repository may hold several projects of a language, such as Cargo projects or
//! TypeScript packages in folders of their own. Each is indexed on its own, and the
//! indexes are put together, so that one that fails leaves the others.

use crate::reader::read_index;
use crate::tools::Tools;
use cyclops_application::ports::{
    IndexError, Indexed, IndexedDocument, Log, SemanticIndex, SemanticProvider,
};
use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::Instant;

/// The TypeScript indexer, pinned, so that every machine runs the same one.
const SCIP_TYPESCRIPT: &str = "@sourcegraph/scip-typescript@0.6.1";

/// The line of what an indexer printed that says why it failed: the first that names an
/// error, such as `Error: no projects`, rather than the end of a stack trace. Without one,
/// the last line, and without any, how it exited.
#[must_use]
pub fn reason_of(printed: &str, exited: &str) -> String {
    let lines: Vec<&str> = printed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .find(|line| {
            let lower = line.to_lowercase();
            lower.contains("error") && !lower.starts_with("npm warn")
        })
        .or_else(|| lines.last())
        .map_or_else(|| exited.to_owned(), |line| (*line).to_owned())
}

/// The folder a file is in, empty for the root.
fn folder_of(path: &RelativePath) -> RelativePath {
    let text = path.as_str();
    RelativePath::new(text.rsplit_once('/').map_or("", |(folder, _)| folder))
}

/// The folders that hold a file of this name, outermost first.
fn folders_with(files: &[RelativePath], name: &str) -> Vec<RelativePath> {
    let mut folders: Vec<RelativePath> = files
        .iter()
        .filter(|path| path.as_str().rsplit('/').next() == Some(name))
        .map(folder_of)
        .collect();
    folders.sort_by(|a, b| {
        depth(a)
            .cmp(&depth(b))
            .then_with(|| a.as_str().cmp(b.as_str()))
    });
    folders.dedup();
    folders
}

fn depth(folder: &RelativePath) -> usize {
    if folder.as_str().is_empty() {
        0
    } else {
        folder.as_str().split('/').count()
    }
}

/// Whether a path is inside a folder, or the folder is the root.
fn is_inside(path: &str, folder: &RelativePath) -> bool {
    let folder = folder.as_str();
    folder.is_empty()
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// A path of a project in a folder, as a path of the whole project. Nothing if it leaves
/// the project.
fn within(folder: &RelativePath, path: &str) -> Option<RelativePath> {
    let mut parts: Vec<&str> = folder
        .as_str()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    Some(RelativePath::new(&parts.join("/")))
}

/// What the index of a project in a folder says, with paths of the whole project.
fn moved(index: SemanticIndex, folder: &RelativePath) -> Vec<IndexedDocument> {
    index
        .documents
        .into_iter()
        .filter_map(|document| {
            let path = within(folder, document.path.as_str())?;
            Some(IndexedDocument { path, ..document })
        })
        .collect()
}

/// How a folder is named in the log: `.` for the root.
fn shown(folder: &RelativePath) -> &str {
    if folder.as_str().is_empty() {
        "."
    } else {
        folder.as_str()
    }
}

/// What both indexers need to run one: the project, where indexes go, the tools, and the
/// log.
#[derive(Clone)]
struct Runner {
    root: PathBuf,
    output: PathBuf,
    tools: Tools,
    log: Arc<dyn Log>,
}

impl Runner {
    /// Runs a program in a folder of the project, and logs it with everything it printed.
    /// Says why it failed if it did.
    fn run(
        &self,
        program: &Path,
        arguments: &[&OsStr],
        folder: &RelativePath,
    ) -> Result<(), String> {
        let name = program
            .file_name()
            .map_or_else(|| program.to_string_lossy(), |name| name.to_string_lossy());
        let words: Vec<String> = arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        let command = format!("{name} {}", words.join(" "));
        self.log
            .write(&format!("run {command} in {}", shown(folder)));
        let started = Instant::now();
        let output = Command::new(program)
            .args(arguments)
            .current_dir(self.root.join(folder.as_str()))
            .env("PATH", self.tools.search_path())
            .stdin(Stdio::null())
            .output();
        let output: Output = match output {
            Ok(output) => output,
            Err(error) => {
                self.log.write(&format!("{name} could not start: {error}"));
                return Err(error.to_string());
            }
        };
        let printed = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        self.log.write(&format!(
            "{name} in {} ended with {} after {:.1} s\n{}",
            shown(folder),
            output.status,
            started.elapsed().as_secs_f64(),
            printed.trim_end()
        ));
        if output.status.success() {
            Ok(())
        } else {
            Err(reason_of(
                &String::from_utf8_lossy(&output.stderr),
                &output.status.to_string(),
            ))
        }
    }

    /// Where an index goes, as a full path, since the indexer may run in a folder.
    fn index_path(&self, name: &str) -> Result<PathBuf, IndexError> {
        std::path::absolute(self.output.join(name)).map_err(|error| {
            IndexError::Failed(format!(
                "{} cannot be written: {error}",
                self.output.display()
            ))
        })
    }

    /// Reads the index an indexer wrote, and removes it, since the cache keeps what
    /// counts of it.
    fn read(&self, path: &Path) -> Result<SemanticIndex, String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
        if let Err(error) = fs::remove_file(path) {
            self.log
                .write(&format!("{} cannot be removed: {error}", path.display()));
        }
        read_index(&bytes).map_err(|error| error.to_string())
    }
}

/// Finds a program, or says that it is missing, under the name people know it by.
fn require(tools: &Tools, program: &str, known_as: &str) -> Result<PathBuf, IndexError> {
    tools
        .find(program)
        .ok_or_else(|| IndexError::Missing(known_as.to_owned()))
}

/// `rust-analyzer scip`, which needs a Cargo toolchain. It runs for each Cargo project,
/// the outermost first, and skips one inside that an index already covers, such as a
/// member of a workspace.
pub struct RustIndexer {
    runner: Runner,
}

impl RustIndexer {
    #[must_use]
    pub fn new(root: &Path, output: &Path, tools: Tools, log: Arc<dyn Log>) -> Self {
        Self {
            runner: Runner {
                root: root.to_path_buf(),
                output: output.to_path_buf(),
                tools,
                log,
            },
        }
    }
}

impl SemanticProvider for RustIndexer {
    fn label(&self) -> &'static str {
        "Rust"
    }

    fn indexer(&self) -> &'static str {
        "rust-analyzer"
    }

    fn languages(&self) -> &[LanguageId] {
        &[LanguageId::Rust]
    }

    fn availability(&self) -> Result<(), IndexError> {
        require(&self.runner.tools, "rust-analyzer", "rust-analyzer")?;
        require(&self.runner.tools, "cargo", "cargo")?;
        Ok(())
    }

    fn index(&self, files: &[RelativePath]) -> Result<Indexed, IndexError> {
        let program = require(&self.runner.tools, "rust-analyzer", "rust-analyzer")?;
        let projects = folders_with(files, "Cargo.toml");
        if projects.is_empty() {
            return Err(IndexError::NoProject("Cargo.toml".to_owned()));
        }
        let mut indexed = Indexed::default();
        for (number, folder) in projects.iter().enumerate() {
            let covered = !folder.as_str().is_empty()
                && indexed
                    .index
                    .documents
                    .iter()
                    .any(|document| is_inside(document.path.as_str(), folder));
            if covered {
                self.runner
                    .log
                    .write(&format!("{} is covered already", shown(folder)));
                continue;
            }
            let output = self.runner.index_path(&format!("rust-{number}.scip"))?;
            let index = self
                .runner
                .run(
                    &program,
                    &[
                        OsStr::new("scip"),
                        OsStr::new(shown(folder)),
                        OsStr::new("--output"),
                        output.as_os_str(),
                    ],
                    &RelativePath::new(""),
                )
                .and_then(|()| self.runner.read(&output));
            match index {
                Ok(index) => indexed.index.documents.extend(moved(index, folder)),
                Err(reason) => indexed.failures.push((folder.clone(), reason)),
            }
        }
        Ok(indexed)
    }
}

/// `scip-typescript`, run through `npx`, which needs Node. It indexes JavaScript too. It
/// runs for each `tsconfig.json` with no other one below it, or once for the whole
/// project when there is none.
pub struct TypeScriptIndexer {
    runner: Runner,
}

impl TypeScriptIndexer {
    #[must_use]
    pub fn new(root: &Path, output: &Path, tools: Tools, log: Arc<dyn Log>) -> Self {
        Self {
            runner: Runner {
                root: root.to_path_buf(),
                output: output.to_path_buf(),
                tools,
                log,
            },
        }
    }
}

/// The folders of TypeScript projects: each with a `tsconfig.json` that has no other one
/// below it.
fn typescript_projects(files: &[RelativePath]) -> Vec<RelativePath> {
    let folders = folders_with(files, "tsconfig.json");
    folders
        .iter()
        .filter(|folder| {
            !folders
                .iter()
                .any(|other| other != *folder && is_inside(other.as_str(), folder))
        })
        .cloned()
        .collect()
}

impl SemanticProvider for TypeScriptIndexer {
    fn label(&self) -> &'static str {
        "TypeScript"
    }

    fn indexer(&self) -> &'static str {
        "scip-typescript"
    }

    fn languages(&self) -> &[LanguageId] {
        &[LanguageId::TypeScript, LanguageId::JavaScript]
    }

    fn availability(&self) -> Result<(), IndexError> {
        require(&self.runner.tools, "node", "Node")?;
        require(&self.runner.tools, "npx", "npx")?;
        Ok(())
    }

    fn index(&self, files: &[RelativePath]) -> Result<Indexed, IndexError> {
        let program = require(&self.runner.tools, "npx", "npx")?;
        let projects = typescript_projects(files);
        let inferred = projects.is_empty();
        let projects = if inferred {
            vec![RelativePath::new("")]
        } else {
            projects
        };
        let mut indexed = Indexed::default();
        for (number, folder) in projects.iter().enumerate() {
            let output = self
                .runner
                .index_path(&format!("typescript-{number}.scip"))?;
            let mut arguments = vec![
                OsStr::new("--yes"),
                OsStr::new(SCIP_TYPESCRIPT),
                OsStr::new("index"),
                OsStr::new("--output"),
                output.as_os_str(),
            ];
            if inferred {
                arguments.push(OsStr::new("--infer-tsconfig"));
            }
            let index = self
                .runner
                .run(&program, &arguments, folder)
                .and_then(|()| self.runner.read(&output));
            match index {
                Ok(index) => indexed.index.documents.extend(moved(index, folder)),
                Err(reason) => indexed.failures.push((folder.clone(), reason)),
            }
        }
        Ok(indexed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(texts: &[&str]) -> Vec<RelativePath> {
        texts.iter().map(|text| RelativePath::new(text)).collect()
    }

    fn texts(paths: &[RelativePath]) -> Vec<&str> {
        paths.iter().map(RelativePath::as_str).collect()
    }

    #[test]
    fn the_reason_is_the_line_that_names_the_error() {
        let rust = "Generating SCIP start...\nError: no projects\n\nStack backtrace:\n   \
                    0: <std::backtrace::Backtrace>::create\n  10: _main\n";
        assert_eq!(reason_of(rust, "exit status: 1"), "Error: no projects");
        let node = "npm warn Unknown project config \"shamefully-hoist\".\n\n<--- Last few GCs \
                    --->\nFATAL ERROR: Reached heap limit Allocation failed - JavaScript heap \
                    out of memory\n----- Native stack trace -----\n 1: 0x1007ac8b4 node::Start\n";
        assert_eq!(
            reason_of(node, "signal: 6"),
            "FATAL ERROR: Reached heap limit Allocation failed - JavaScript heap out of memory"
        );
    }

    #[test]
    fn without_an_error_line_the_reason_is_the_last_line_or_how_it_exited() {
        assert_eq!(
            reason_of("working\nit broke\n", "exit status: 1"),
            "it broke"
        );
        assert_eq!(reason_of("\n  \n", "exit status: 1"), "exit status: 1");
    }

    #[test]
    fn cargo_projects_go_outermost_first() {
        let files = paths(&[
            "tools/cli/Cargo.toml",
            "Cargo.toml",
            "services/api/Cargo.toml",
            "services/api/src/main.rs",
            "services/api/evaluators/Cargo.toml",
        ]);
        assert_eq!(
            texts(&folders_with(&files, "Cargo.toml")),
            vec!["", "services/api", "tools/cli", "services/api/evaluators"]
        );
    }

    #[test]
    fn a_typescript_project_is_a_tsconfig_with_none_below_it() {
        let files = paths(&[
            "tsconfig.json",
            "web/shop/tsconfig.json",
            "web/shop/src/orders.ts",
            "packages/ui/tsconfig.json",
            "packages/ui/stories/tsconfig.json",
        ]);
        assert_eq!(
            texts(&typescript_projects(&files)),
            vec!["web/shop", "packages/ui/stories"]
        );
        assert_eq!(
            texts(&typescript_projects(&paths(&["tsconfig.json", "src/a.ts"]))),
            vec![""]
        );
        assert_eq!(
            texts(&typescript_projects(&paths(&["src/a.ts"]))),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn a_path_in_a_folder_becomes_a_path_of_the_project() {
        let folder = RelativePath::new("services/api");
        assert_eq!(
            within(&folder, "src/main.rs").map(|path| path.to_string()),
            Some("services/api/src/main.rs".to_owned())
        );
        assert_eq!(
            within(&folder, "../shared/lib.rs").map(|path| path.to_string()),
            Some("services/shared/lib.rs".to_owned())
        );
        assert_eq!(within(&RelativePath::new(""), "../outside.rs"), None);
        assert_eq!(
            within(&RelativePath::new(""), "src/lib.rs").map(|path| path.to_string()),
            Some("src/lib.rs".to_owned())
        );
    }

    #[test]
    fn a_folder_holds_what_is_below_it_and_the_root_holds_everything() {
        let folder = RelativePath::new("services/api");
        assert!(is_inside("services/api/src/main.rs", &folder));
        assert!(!is_inside("services/apiary/src/main.rs", &folder));
        assert!(is_inside("anything.rs", &RelativePath::new("")));
    }
}
