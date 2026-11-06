//! A throwaway copy of a project for one row of a table.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A small valid source file, for the `add` step.
const SMALL_FILE: &str = "// Added by a step.\n";

pub struct Scratch {
    root: PathBuf,
}

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../samples")
}

fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_folder(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

impl Scratch {
    fn empty() -> io::Result<Self> {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("cyclops-spec-{}-{id}", process::id());
        let root = std::env::temp_dir().join(name);
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// A fresh project made of one file.
    pub fn with_file(manifest: Option<&str>, path: &str, code: &str) -> io::Result<Self> {
        let scratch = Self::empty()?;
        if let Some(manifest) = manifest {
            scratch.write("Cargo.toml", manifest.as_bytes())?;
        }
        scratch.write(path, code.as_bytes())?;
        Ok(scratch)
    }

    /// A fresh copy of a sample workspace.
    pub fn from_sample(workspace: &str) -> io::Result<Self> {
        let scratch = Self::empty()?;
        copy_folder(&samples().join(workspace), &scratch.root)?;
        Ok(scratch)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, path: &str, contents: &[u8]) -> io::Result<()> {
        let target = self.root.join(path);
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(target, contents)
    }

    /// Takes one step. See `specs/writing-specs.md`.
    pub fn step(&self, step: &str) -> Result<(), String> {
        let done = if let Some(path) = step.strip_prefix("add binary ") {
            self.write(path, &[0xff, 0xfe, 0x00])
        } else if let Some(path) = step.strip_prefix("add ") {
            self.write(path, SMALL_FILE.as_bytes())
        } else if let Some(path) = step.strip_prefix("lock ") {
            fs::set_permissions(self.root.join(path), fs::Permissions::from_mode(0o000))
        } else if let Some(pattern) = step.strip_prefix("ignore ") {
            self.write(".gitignore", format!("{pattern}\n").as_bytes())
        } else {
            return Err(format!("unknown step: {step}"));
        };
        done.map_err(|error| format!("{step}: {error}"))
    }
}

impl Drop for Scratch {
    /// A leftover folder in the temporary folder does no harm, so a failure here must
    /// not fail the row that has already been read.
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("{}: {error}", self.root.display());
        }
    }
}
