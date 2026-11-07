//! A throwaway project, and a home folder, for one row of a table.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

pub struct Scratch {
    base: PathBuf,
    root: PathBuf,
    home: PathBuf,
}

pub fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
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
    /// An empty project named `name`, with an empty home folder beside it.
    fn empty(name: &str) -> io::Result<Self> {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!("cyclops-spec-{}-{id}", process::id()));
        let root = base.join(name);
        let home = base.join("home");
        fs::create_dir_all(&root)?;
        fs::create_dir_all(&home)?;
        Ok(Self { base, root, home })
    }

    /// A fresh project made of one file.
    pub fn with_file(manifest: Option<&str>, path: &str, code: &str) -> io::Result<Self> {
        let scratch = Self::empty("snippet")?;
        if let Some(manifest) = manifest {
            scratch.write("Cargo.toml", manifest.as_bytes())?;
        }
        scratch.write(path, code.as_bytes())?;
        Ok(scratch)
    }

    /// A fresh copy of a sample workspace.
    pub fn from_sample(workspace: &str) -> io::Result<Self> {
        let scratch = Self::empty(workspace)?;
        copy_folder(&repository().join("samples").join(workspace), &scratch.root)?;
        Ok(scratch)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn write(&self, path: &str, contents: &[u8]) -> io::Result<()> {
        let target = self.root.join(path);
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(target, contents)
    }

    /// Makes a file unreadable.
    pub fn lock(&self, path: &str) -> io::Result<()> {
        fs::set_permissions(self.root.join(path), fs::Permissions::from_mode(0o000))
    }
}

impl Drop for Scratch {
    /// A leftover folder in the temporary folder does no harm, so a failure here must
    /// not fail the row that has already been read.
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.base) {
            eprintln!("{}: {error}", self.base.display());
        }
    }
}
