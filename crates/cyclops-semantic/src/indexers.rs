//! The indexers Cyclops knows, and how to run them.
//!
//! A repository may hold several projects of a language, such as Cargo workspaces or
//! TypeScript packages in folders of their own. Each is indexed on its own, and the
//! indexes are put together, so that one that fails leaves the others.

use crate::reader::read_rooted;
use crate::tools::Tools;
use cyclops_application::ports::{
    IndexError, Indexed, IndexedDocument, Log, SemanticIndex, SemanticProvider,
};
use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use std::collections::HashSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};
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

/// A path with `.` and `..` worked out, without looking at the disk.
fn tidied(path: &Path) -> PathBuf {
    let mut tidy = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                tidy.pop();
            }
            other => tidy.push(other),
        }
    }
    tidy
}

/// The folder a `file://` URI names, with `%XX` escapes decoded.
fn folder_of_uri(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(byte) = bytes.get(at) {
        let escaped = (*byte == b'%')
            .then(|| bytes.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(escaped) = escaped {
            decoded.push(escaped);
            at += 3;
        } else {
            decoded.push(*byte);
            at += 1;
        }
    }
    let path = String::from_utf8(decoded).ok()?;
    let path = path.trim_end_matches('/');
    Some(PathBuf::from(if path.is_empty() { "/" } else { path }))
}

/// A file an index names, relative to `base`, as a path of the project at `project`.
/// Nothing if it is outside the project. Both folders are as the disk names them.
fn in_project(project: &Path, base: &Path, path: &str) -> Option<RelativePath> {
    let full = tidied(&base.join(path));
    let inside = full.strip_prefix(project).ok()?;
    let parts: Vec<String> = inside
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(RelativePath::new(&parts.join("/")))
}

/// What an index says, with paths of the project, leaving out files outside it.
fn placed(index: SemanticIndex, project: &Path, base: &Path) -> Vec<IndexedDocument> {
    index
        .documents
        .into_iter()
        .filter_map(|document| {
            let path = in_project(project, base, document.path.as_str())?;
            Some(IndexedDocument { path, ..document })
        })
        .collect()
}

/// A folder as the disk names it, following links, so that two names for one folder
/// agree.
fn real(folder: &Path) -> PathBuf {
    fs::canonicalize(folder).unwrap_or_else(|_| tidied(folder))
}

/// Adds what a project's index says to the whole index. A file two projects both
/// indexed keeps what the first said.
fn merge(index: &mut SemanticIndex, documents: Vec<IndexedDocument>) {
    let known: HashSet<RelativePath> = index
        .documents
        .iter()
        .map(|document| document.path.clone())
        .collect();
    index.documents.extend(
        documents
            .into_iter()
            .filter(|document| !known.contains(&document.path)),
    );
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
        variables: &[(&str, OsString)],
    ) -> Result<String, String> {
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
            .envs(variables.iter().map(|(name, value)| (*name, value)))
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
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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

    /// Reads the index an indexer wrote, with paths of the project, and removes it, since
    /// the cache keeps what counts of it. Its paths are relative to the folder it says,
    /// or else to `ran_in`.
    fn read(
        &self,
        path: &Path,
        project: &Path,
        ran_in: &Path,
    ) -> Result<Vec<IndexedDocument>, String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
        if let Err(error) = fs::remove_file(path) {
            self.log
                .write(&format!("{} cannot be removed: {error}", path.display()));
        }
        let (index, root) = read_rooted(&bytes).map_err(|error| error.to_string())?;
        let base = folder_of_uri(&root).map_or_else(|| real(ran_in), |root| real(&root));
        Ok(placed(index, project, &base))
    }

    /// The project's folder, as the disk names it.
    fn project(&self) -> PathBuf {
        real(&self.root)
    }
}

/// Finds a program, or says that it is missing, under the name people know it by.
fn require(tools: &Tools, program: &str, known_as: &str) -> Result<PathBuf, IndexError> {
    tools
        .find(program)
        .ok_or_else(|| IndexError::Missing(known_as.to_owned()))
}

/// `rust-analyzer scip`, which needs a Cargo toolchain. It runs once for each Cargo
/// workspace of the project, as Cargo finds them, from the workspace's root, so that the
/// whole workspace is indexed, even when its root is outside the project.
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

    /// The workspaces of the Cargo projects in these folders, as Cargo finds them, in
    /// order, and each folder it could not find one for, with why.
    fn workspaces(
        &self,
        cargo: &Path,
        folders: &[RelativePath],
    ) -> (Vec<PathBuf>, Vec<(RelativePath, String)>) {
        let mut found: Vec<PathBuf> = Vec::new();
        let mut failures = Vec::new();
        for folder in folders {
            let located = self.runner.run(
                cargo,
                &[
                    OsStr::new("locate-project"),
                    OsStr::new("--workspace"),
                    OsStr::new("--message-format"),
                    OsStr::new("plain"),
                ],
                folder,
                &[],
            );
            match located {
                Ok(manifest) => {
                    let manifest = PathBuf::from(manifest.trim());
                    let workspace = real(manifest.parent().unwrap_or(&manifest));
                    if !found.contains(&workspace) {
                        found.push(workspace);
                    }
                }
                Err(reason) => failures.push((folder.clone(), reason)),
            }
        }
        found.sort();
        (found, failures)
    }
}

/// How a folder is named in a problem: as a folder of the project, or in full when it
/// is outside it.
fn named(project: &Path, folder: &Path) -> RelativePath {
    folder.strip_prefix(project).map_or_else(
        |_| RelativePath::new(&folder.to_string_lossy()),
        |inside| RelativePath::new(&inside.to_string_lossy()),
    )
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
        let cargo = require(&self.runner.tools, "cargo", "cargo")?;
        let manifests = folders_with(files, "Cargo.toml");
        if manifests.is_empty() {
            return Err(IndexError::NoProject("Cargo.toml".to_owned()));
        }
        let project = self.runner.project();
        let (workspaces, failures) = self.workspaces(&cargo, &manifests);
        let mut indexed = Indexed {
            failures,
            ..Indexed::default()
        };
        for (number, workspace) in workspaces.iter().enumerate() {
            let output = self.runner.index_path(&format!("rust-{number}.scip"))?;
            let documents = self
                .runner
                .run(
                    &program,
                    &[
                        OsStr::new("scip"),
                        workspace.as_os_str(),
                        OsStr::new("--output"),
                        output.as_os_str(),
                    ],
                    &RelativePath::new(""),
                    &[],
                )
                .and_then(|_| self.runner.read(&output, &project, workspace));
            match documents {
                Ok(documents) => merge(&mut indexed.index, documents),
                Err(reason) => indexed.failures.push((named(&project, workspace), reason)),
            }
        }
        Ok(indexed)
    }
}

/// `scip-typescript`, run through `npx`, which needs Node. It indexes JavaScript too. It
/// runs for each `tsconfig.json` but an empty one among others, or once for the whole
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

/// The folders of TypeScript projects: each with a `tsconfig.json`, except one that is
/// empty when there are others, since such a file only serves editors and would take in
/// every package at once. `is_empty` says whether a folder's `tsconfig.json` is.
fn typescript_projects(
    files: &[RelativePath],
    is_empty: impl Fn(&RelativePath) -> bool,
) -> Vec<RelativePath> {
    let folders = folders_with(files, "tsconfig.json");
    if folders.len() < 2 {
        return folders;
    }
    folders
        .into_iter()
        .filter(|folder| !is_empty(folder))
        .collect()
}

/// Whether a `tsconfig.json` sets nothing: once its comments are gone, it is `{}` or
/// nothing at all.
fn sets_nothing(text: &str) -> bool {
    let mut kept = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("//") {
            rest = after.split_once('\n').map_or("", |(_, next)| next);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, next)| next);
        } else {
            let mut letters = rest.chars();
            if let Some(letter) = letters.next()
                && !letter.is_whitespace()
            {
                kept.push(letter);
            }
            rest = letters.as_str();
        }
    }
    kept.is_empty() || kept == "{}"
}

/// How much memory Node may use for an index: as much as a large package needs. Options
/// already set come after, so that they win.
fn node_options(set: Option<&OsStr>) -> OsString {
    let mut options = OsString::from("--max-old-space-size=16384");
    if let Some(set) = set.filter(|set| !set.is_empty()) {
        options.push(" ");
        options.push(set);
    }
    options
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
        let root = self.runner.root.clone();
        let projects = typescript_projects(files, |folder| {
            fs::read_to_string(root.join(folder.as_str()).join("tsconfig.json"))
                .is_ok_and(|text| sets_nothing(&text))
        });
        let project = self.runner.project();
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
            let node = node_options(env::var_os("NODE_OPTIONS").as_deref());
            let ran_in = self.runner.root.join(folder.as_str());
            let documents = self
                .runner
                .run(&program, &arguments, folder, &[("NODE_OPTIONS", node)])
                .and_then(|_| self.runner.read(&output, &project, &ran_in));
            match documents {
                Ok(documents) => merge(&mut indexed.index, documents),
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
    fn a_typescript_project_is_any_tsconfig_but_an_empty_one_among_others() {
        let files = paths(&[
            "tsconfig.json",
            "web/shop/tsconfig.json",
            "web/shop/src/orders.ts",
            "apps/server/tsconfig.json",
            "apps/server/specification/tsconfig.json",
        ]);
        let root_is_empty = |folder: &RelativePath| folder.as_str().is_empty();
        assert_eq!(
            texts(&typescript_projects(&files, root_is_empty)),
            vec!["apps/server", "web/shop", "apps/server/specification"]
        );
        let none_is_empty = |_: &RelativePath| false;
        assert_eq!(
            texts(&typescript_projects(
                &paths(&["tsconfig.json", "specification/tsconfig.json"]),
                none_is_empty
            )),
            vec!["", "specification"]
        );
        assert_eq!(
            texts(&typescript_projects(
                &paths(&["tsconfig.json"]),
                root_is_empty
            )),
            vec![""]
        );
        assert_eq!(
            texts(&typescript_projects(&paths(&["src/a.ts"]), root_is_empty)),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn a_tsconfig_that_sets_nothing_is_empty() {
        assert!(sets_nothing("{}\n"));
        assert!(sets_nothing("// Added by a step.\n"));
        assert!(sets_nothing("/* editors only */ { }"));
        assert!(sets_nothing(""));
        assert!(!sets_nothing("{ \"include\": [\"src\"] }"));
        assert!(!sets_nothing(
            "// a comment\n{ \"extends\": \"../base.json\" }"
        ));
    }

    #[test]
    fn a_file_two_projects_indexed_keeps_what_the_first_said() {
        let document = |path: &str, line: u32| IndexedDocument {
            path: RelativePath::new(path),
            definitions: Vec::new(),
            references: vec![cyclops_application::ports::IndexedReference {
                line,
                key: "k".to_owned(),
                owner: None,
            }],
        };
        let mut index = SemanticIndex {
            documents: vec![document("apps/server/specification/a.ts", 1)],
        };
        merge(
            &mut index,
            vec![
                document("apps/server/specification/a.ts", 2),
                document("apps/server/src/b.ts", 3),
            ],
        );
        let lines: Vec<(&str, u32)> = index
            .documents
            .iter()
            .flat_map(|document| {
                document
                    .references
                    .iter()
                    .map(move |reference| (document.path.as_str(), reference.line))
            })
            .collect();
        assert_eq!(
            lines,
            vec![
                ("apps/server/specification/a.ts", 1),
                ("apps/server/src/b.ts", 3)
            ]
        );
    }

    #[test]
    fn node_gets_room_unless_its_options_say_otherwise() {
        assert_eq!(node_options(None), "--max-old-space-size=16384");
        assert_eq!(
            node_options(Some(OsStr::new(""))),
            "--max-old-space-size=16384"
        );
        assert_eq!(
            node_options(Some(OsStr::new("--max-old-space-size=4096"))),
            "--max-old-space-size=16384 --max-old-space-size=4096"
        );
    }

    #[test]
    fn a_path_an_index_names_becomes_a_path_of_the_project() {
        let project = Path::new("/work/core/apps/server");
        let in_it = |base: &str, path: &str| {
            in_project(project, Path::new(base), path).map(|path| path.to_string())
        };
        assert_eq!(
            in_it("/work/core/apps/server", "src/main.ts"),
            Some("src/main.ts".to_owned())
        );
        assert_eq!(
            in_it("/work/core/apps/server/specification", "../src/main.ts"),
            Some("src/main.ts".to_owned())
        );
        assert_eq!(
            in_it("/work/core", "apps/server/src/lib.rs"),
            Some("src/lib.rs".to_owned())
        );
        assert_eq!(in_it("/work/core", "apps/web/src/lib.rs"), None);
    }

    #[test]
    fn a_file_uri_names_a_folder() {
        assert_eq!(
            folder_of_uri("file:///work/my%20core/"),
            Some(PathBuf::from("/work/my core"))
        );
        assert_eq!(folder_of_uri("file:///"), Some(PathBuf::from("/")));
        assert_eq!(folder_of_uri(""), None);
    }
}
