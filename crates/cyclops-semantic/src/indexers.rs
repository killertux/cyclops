//! The indexers Cyclops knows, and how to run them.

use crate::reader::read_index;
use crate::tools::Tools;
use cyclops_application::ports::{IndexError, SemanticIndex, SemanticProvider};
use cyclops_domain::language::LanguageId;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The TypeScript indexer, pinned, so that every machine runs the same one.
const SCIP_TYPESCRIPT: &str = "@sourcegraph/scip-typescript@0.6.1";

/// Runs a program in the project, and says why it failed if it did. What it prints is
/// kept from the person, who only needs to know whether it worked.
fn run(program: &Path, arguments: &[&OsStr], root: &Path, tools: &Tools) -> Result<(), IndexError> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .env("PATH", tools.search_path())
        .stdin(Stdio::null())
        .output()
        .map_err(|error| IndexError::Failed(error.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let printed = String::from_utf8_lossy(&output.stderr);
    let reason = printed
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map_or_else(|| output.status.to_string(), str::to_owned);
    Err(IndexError::Failed(reason))
}

/// Reads the index an indexer wrote.
fn read_from(path: &Path) -> Result<SemanticIndex, IndexError> {
    let bytes = fs::read(path).map_err(|error| {
        IndexError::Failed(format!("{} cannot be read: {error}", path.display()))
    })?;
    read_index(&bytes)
}

/// Finds a program, or says that it is missing, under the name people know it by.
fn require(tools: &Tools, program: &str, known_as: &str) -> Result<PathBuf, IndexError> {
    tools
        .find(program)
        .ok_or_else(|| IndexError::Missing(known_as.to_owned()))
}

/// `rust-analyzer scip`, which needs a Cargo toolchain.
pub struct RustIndexer {
    root: PathBuf,
    output: PathBuf,
    tools: Tools,
}

impl RustIndexer {
    #[must_use]
    pub fn new(root: &Path, output: &Path, tools: Tools) -> Self {
        Self {
            root: root.to_path_buf(),
            output: output.join("rust.scip"),
            tools,
        }
    }
}

impl SemanticProvider for RustIndexer {
    fn label(&self) -> &'static str {
        "Rust"
    }

    fn languages(&self) -> &[LanguageId] {
        &[LanguageId::Rust]
    }

    fn availability(&self) -> Result<(), IndexError> {
        require(&self.tools, "rust-analyzer", "rust-analyzer")?;
        require(&self.tools, "cargo", "cargo")?;
        Ok(())
    }

    fn index(&self) -> Result<SemanticIndex, IndexError> {
        let program = require(&self.tools, "rust-analyzer", "rust-analyzer")?;
        run(
            &program,
            &[
                OsStr::new("scip"),
                OsStr::new("."),
                OsStr::new("--output"),
                self.output.as_os_str(),
            ],
            &self.root,
            &self.tools,
        )?;
        read_from(&self.output)
    }
}

/// `scip-typescript`, run through `npx`, which needs Node. It indexes JavaScript too.
pub struct TypeScriptIndexer {
    root: PathBuf,
    output: PathBuf,
    tools: Tools,
}

impl TypeScriptIndexer {
    #[must_use]
    pub fn new(root: &Path, output: &Path, tools: Tools) -> Self {
        Self {
            root: root.to_path_buf(),
            output: output.join("typescript.scip"),
            tools,
        }
    }
}

impl SemanticProvider for TypeScriptIndexer {
    fn label(&self) -> &'static str {
        "TypeScript"
    }

    fn languages(&self) -> &[LanguageId] {
        &[LanguageId::TypeScript, LanguageId::JavaScript]
    }

    fn availability(&self) -> Result<(), IndexError> {
        require(&self.tools, "node", "Node")?;
        require(&self.tools, "npx", "npx")?;
        Ok(())
    }

    fn index(&self) -> Result<SemanticIndex, IndexError> {
        let program = require(&self.tools, "npx", "npx")?;
        let mut arguments = vec![
            OsStr::new("--yes"),
            OsStr::new(SCIP_TYPESCRIPT),
            OsStr::new("index"),
            OsStr::new("--output"),
            self.output.as_os_str(),
        ];
        if !self.root.join("tsconfig.json").is_file() {
            arguments.push(OsStr::new("--infer-tsconfig"));
        }
        run(&program, &arguments, &self.root, &self.tools)?;
        read_from(&self.output)
    }
}
