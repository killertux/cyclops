//! The fact cache, in `.cyclops/cache/cyclops.db`. It is disposable: anything wrong with
//! it is fixed by starting it over.

use cyclops_application::ports::{
    CacheEntry, CacheError, CachedFacts, Commit, ContentHash, FactStore, FileChange, FileStamp,
    KnownHistory, NewHistory, StoredIndex,
};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::SymbolId;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Raise it whenever the layout of the database or of the facts changes. A cache of
/// another version is started over.
pub const SCHEMA_VERSION: i64 = 5;

pub struct SqliteFactStore {
    connection: Connection,
}

fn failed(error: impl std::fmt::Display) -> CacheError {
    CacheError(error.to_string())
}

fn create(path: &Path) -> Result<Connection, CacheError> {
    let connection = Connection::open(path).map_err(failed)?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                adapter TEXT NOT NULL,
                version INTEGER NOT NULL,
                size INTEGER NOT NULL,
                modified INTEGER NOT NULL,
                hash BLOB NOT NULL,
                facts BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS commits (
                position INTEGER PRIMARY KEY,
                id TEXT NOT NULL,
                time INTEGER NOT NULL,
                subject TEXT NOT NULL,
                changes BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS head (id TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS indexes (
                label TEXT PRIMARY KEY,
                contents BLOB NOT NULL
            );",
        )
        .map_err(failed)?;
    connection
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(failed)?;
    Ok(connection)
}

fn version_of(connection: &Connection) -> Result<i64, rusqlite::Error> {
    connection.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Opens a database that is ours and current, or starts one over.
fn open_or_restart(path: &Path) -> Result<Connection, CacheError> {
    let current = Connection::open(path)
        .ok()
        .filter(|connection| version_of(connection).is_ok_and(|version| version == SCHEMA_VERSION));
    if let Some(connection) = current {
        return Ok(connection);
    }
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(failed(error)),
    }
    create(path)
}

/// An index the way the cache keeps it.
#[derive(Serialize, Deserialize)]
struct KeptIndex {
    languages: Vec<LanguageId>,
    files: Vec<(RelativePath, ContentHash)>,
    covered: Vec<RelativePath>,
    uses: Vec<(String, String)>,
}

impl KeptIndex {
    fn of(index: &StoredIndex) -> Self {
        Self {
            languages: index.languages.clone(),
            files: index.files.clone(),
            covered: index.covered.clone(),
            uses: index
                .uses
                .iter()
                .map(|(from, to)| (from.as_str().to_owned(), to.as_str().to_owned()))
                .collect(),
        }
    }

    fn into_index(self, label: String) -> StoredIndex {
        StoredIndex {
            label,
            languages: self.languages,
            files: self.files,
            covered: self.covered,
            uses: self
                .uses
                .into_iter()
                .map(|(from, to)| (SymbolId::new(&from), SymbolId::new(&to)))
                .collect(),
        }
    }
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

impl SqliteFactStore {
    /// Opens the cache, starting it over if it is missing, damaged or out of date.
    ///
    /// # Errors
    ///
    /// Fails if no cache can be made at that place.
    pub fn open(path: &Path) -> Result<Self, CacheError> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(failed)?;
        }
        Ok(Self {
            connection: open_or_restart(path)?,
        })
    }

    fn read_all(&self) -> Result<HashMap<RelativePath, CachedFacts>, rusqlite::Error> {
        let mut statement = self
            .connection
            .prepare("SELECT path, adapter, version, size, modified, hash, facts FROM files")?;
        let rows = statement.query_map([], |row| {
            let path: String = row.get(0)?;
            let hash: Vec<u8> = row.get(5)?;
            let facts: Vec<u8> = row.get(6)?;
            Ok((
                path,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                FileStamp {
                    size: to_u64(row.get(3)?),
                    modified: to_u64(row.get(4)?),
                },
                hash,
                facts,
            ))
        })?;
        let mut known = HashMap::new();
        for row in rows {
            let (path, adapter, version, stamp, hash, facts) = row?;
            // A row that cannot be read is only a file to parse again.
            let (Ok(hash), Ok(facts)) = (
                <[u8; 32]>::try_from(hash.as_slice()),
                postcard::from_bytes::<FileFacts>(&facts),
            ) else {
                continue;
            };
            let entry = CachedFacts {
                adapter,
                version,
                stamp,
                hash,
                facts,
            };
            known.insert(RelativePath::new(&path), entry);
        }
        Ok(known)
    }
}

/// A commit's changes as the cache keeps them.
type StoredChanges = Vec<(String, Option<String>)>;

impl SqliteFactStore {
    fn read_history(&self) -> Result<KnownHistory, rusqlite::Error> {
        let head = self
            .connection
            .query_row("SELECT id FROM head", [], |row| row.get::<_, String>(0))
            .ok();
        let mut statement = self
            .connection
            .prepare("SELECT id, time, subject, changes FROM commits ORDER BY position DESC")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })?;
        let mut commits = Vec::new();
        for row in rows {
            let (id, time, subject, changes) = row?;
            // History that cannot be read is read again from git.
            let Ok(changes) = postcard::from_bytes::<StoredChanges>(&changes) else {
                return Ok(KnownHistory::default());
            };
            let changes = changes
                .into_iter()
                .map(|(path, previous)| FileChange { path, previous })
                .collect();
            commits.push(Commit {
                id,
                time,
                subject,
                changes,
            });
        }
        Ok(KnownHistory { head, commits })
    }

    fn write_history(&mut self, history: &NewHistory) -> Result<(), CacheError> {
        let transaction = self.connection.transaction().map_err(failed)?;
        if history.rewritten {
            transaction
                .execute("DELETE FROM commits", [])
                .map_err(failed)?;
        }
        let last: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(position), 0) FROM commits",
                [],
                |row| row.get(0),
            )
            .map_err(failed)?;
        {
            let mut insert = transaction
                .prepare(
                    "INSERT INTO commits (position, id, time, subject, changes) VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(failed)?;
            // The newest commit comes first and gets the highest position.
            let count = i64::try_from(history.commits.len()).unwrap_or(i64::MAX);
            for (offset, commit) in (0..count).rev().zip(&history.commits) {
                let changes: StoredChanges = commit
                    .changes
                    .iter()
                    .map(|change| (change.path.clone(), change.previous.clone()))
                    .collect();
                let bytes = postcard::to_stdvec(&changes).map_err(failed)?;
                insert
                    .execute(params![
                        last + offset + 1,
                        commit.id,
                        commit.time,
                        commit.subject,
                        bytes
                    ])
                    .map_err(failed)?;
            }
        }
        transaction
            .execute("DELETE FROM head", [])
            .map_err(failed)?;
        transaction
            .execute("INSERT INTO head (id) VALUES (?1)", params![history.head])
            .map_err(failed)?;
        transaction.commit().map_err(failed)
    }
}

impl FactStore for SqliteFactStore {
    fn load_indexes(&mut self) -> Result<Vec<StoredIndex>, CacheError> {
        let mut statement = self
            .connection
            .prepare("SELECT label, contents FROM indexes ORDER BY label")
            .map_err(failed)?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get::<_, Vec<u8>>(1)?)))
            .map_err(failed)?;
        let mut indexes = Vec::new();
        for row in rows {
            let (label, contents) = row.map_err(failed)?;
            let kept: KeptIndex = postcard::from_bytes(&contents).map_err(failed)?;
            indexes.push(kept.into_index(label));
        }
        Ok(indexes)
    }

    fn save_index(&mut self, index: &StoredIndex) -> Result<(), CacheError> {
        let contents = postcard::to_stdvec(&KeptIndex::of(index)).map_err(failed)?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO indexes (label, contents) VALUES (?1, ?2)",
                params![index.label, contents],
            )
            .map_err(failed)?;
        Ok(())
    }

    fn load_history(&mut self) -> Result<KnownHistory, CacheError> {
        self.read_history().map_err(failed)
    }

    fn save_history(&mut self, history: &NewHistory) -> Result<(), CacheError> {
        self.write_history(history)
    }

    fn load(&mut self) -> Result<HashMap<RelativePath, CachedFacts>, CacheError> {
        self.read_all().map_err(failed)
    }

    fn save(
        &mut self,
        changed: &[CacheEntry<'_>],
        gone: &[RelativePath],
    ) -> Result<(), CacheError> {
        let transaction = self.connection.transaction().map_err(failed)?;
        {
            let mut upsert = transaction
                .prepare(
                    "INSERT OR REPLACE INTO files (path, adapter, version, size, modified, hash, facts)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(failed)?;
            for entry in changed {
                let facts = postcard::to_stdvec(entry.facts).map_err(failed)?;
                upsert
                    .execute(params![
                        entry.facts.path.as_str(),
                        entry.adapter,
                        entry.version,
                        to_i64(entry.stamp.size),
                        to_i64(entry.stamp.modified),
                        entry.hash.as_slice(),
                        facts,
                    ])
                    .map_err(failed)?;
            }
            let mut delete = transaction
                .prepare("DELETE FROM files WHERE path = ?1")
                .map_err(failed)?;
            for path in gone {
                delete.execute(params![path.as_str()]).map_err(failed)?;
            }
        }
        transaction.commit().map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Where a test can put a cache of its own.
    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cyclops-cache-{}-{name}", std::process::id()))
    }
    use cyclops_domain::facts::LineCounts;
    use cyclops_domain::language::LanguageId;
    use cyclops_domain::symbol::CodeRole;

    fn facts(path: &str) -> FileFacts {
        FileFacts {
            path: RelativePath::new(path),
            language: LanguageId::Rust,
            role: CodeRole::Production,
            lines: LineCounts::default(),
            test_lines: LineCounts::default(),
            symbols: Vec::new(),
            imports: Vec::new(),
            syntax_error: false,
            test_modules: Vec::new(),
            extensions: Vec::new(),
            exports: Vec::new(),
            tokens: Vec::new(),
        }
    }

    fn entry(facts: &FileFacts) -> CacheEntry<'_> {
        CacheEntry {
            adapter: "rust",
            version: 1,
            stamp: FileStamp {
                size: 1,
                modified: 2,
            },
            hash: [7; 32],
            facts,
        }
    }

    #[test]
    fn what_is_saved_comes_back_and_what_is_gone_does_not() {
        let folder = scratch("round-trip");
        let path = folder.join("cyclops.db");
        let (a, b) = (facts("a.rs"), facts("b.rs"));
        {
            let mut store = SqliteFactStore::open(&path).expect("opened");
            store.save(&[entry(&a), entry(&b)], &[]).expect("saved");
            store
                .save(&[], &[RelativePath::new("b.rs")])
                .expect("saved");
        }
        let known = SqliteFactStore::open(&path)
            .expect("opened")
            .load()
            .expect("loaded");
        fs::remove_dir_all(&folder).expect("removed");
        assert_eq!(known.len(), 1);
        let cached = known.get(&RelativePath::new("a.rs")).expect("cached");
        assert_eq!(cached.facts, a);
        assert_eq!(cached.hash, [7; 32]);
    }

    fn commit(id: &str, time: i64) -> Commit {
        Commit {
            id: id.to_owned(),
            time,
            subject: format!("Change {id}"),
            changes: vec![FileChange {
                path: "b.rs".to_owned(),
                previous: Some("a.rs".to_owned()),
            }],
        }
    }

    #[test]
    fn history_grows_newest_first_and_starts_over_when_rewritten() {
        let folder = scratch("history");
        let path = folder.join("cyclops.db");
        let mut store = SqliteFactStore::open(&path).expect("opened");
        let first = NewHistory {
            head: "b".to_owned(),
            commits: vec![commit("b", 2), commit("a", 1)],
            rewritten: false,
        };
        store.save_history(&first).expect("saved");
        let more = NewHistory {
            head: "c".to_owned(),
            commits: vec![commit("c", 3)],
            rewritten: false,
        };
        store.save_history(&more).expect("saved");
        let known = store.load_history().expect("loaded");
        let ids: Vec<&str> = known
            .commits
            .iter()
            .map(|commit| commit.id.as_str())
            .collect();
        assert_eq!(known.head.as_deref(), Some("c"));
        assert_eq!(ids, ["c", "b", "a"]);
        assert_eq!(
            known.commits.first().map(|commit| commit.changes.clone()),
            Some(commit("c", 3).changes)
        );

        let rewritten = NewHistory {
            head: "d".to_owned(),
            commits: vec![commit("d", 4)],
            rewritten: true,
        };
        store.save_history(&rewritten).expect("saved");
        let known = store.load_history().expect("loaded");
        fs::remove_dir_all(&folder).expect("removed");
        assert_eq!(known.commits.len(), 1);
    }

    #[test]
    fn garbage_is_started_over() {
        let folder = scratch("garbage");
        let path = folder.join("cyclops.db");
        fs::create_dir_all(&folder).expect("a folder");
        fs::write(&path, "not a database at all, not even close").expect("written");
        let known = SqliteFactStore::open(&path)
            .expect("opened")
            .load()
            .expect("loaded");
        fs::remove_dir_all(&folder).expect("removed");
        assert!(known.is_empty());
    }

    #[test]
    fn an_older_schema_is_started_over() {
        let folder = scratch("older");
        let path = folder.join("cyclops.db");
        let a = facts("a.rs");
        {
            let mut store = SqliteFactStore::open(&path).expect("opened");
            store.save(&[entry(&a)], &[]).expect("saved");
            store
                .connection
                .pragma_update(None, "user_version", SCHEMA_VERSION - 1)
                .expect("outdated");
        }
        let known = SqliteFactStore::open(&path)
            .expect("opened")
            .load()
            .expect("loaded");
        fs::remove_dir_all(&folder).expect("removed");
        assert!(known.is_empty());
    }

    #[test]
    fn a_row_that_cannot_be_read_is_skipped() {
        let folder = scratch("bad-row");
        let path = folder.join("cyclops.db");
        let a = facts("a.rs");
        let mut store = SqliteFactStore::open(&path).expect("opened");
        store.save(&[entry(&a)], &[]).expect("saved");
        store
            .connection
            .execute("UPDATE files SET facts = x'ff'", [])
            .expect("damaged");
        let known = store.load().expect("loaded");
        fs::remove_dir_all(&folder).expect("removed");
        assert!(known.is_empty());
    }

    fn stored(label: &str, uses: &[(&str, &str)]) -> StoredIndex {
        StoredIndex {
            label: label.to_owned(),
            languages: vec![LanguageId::Rust],
            files: vec![(RelativePath::new("src/a.rs"), [3; 32])],
            covered: vec![RelativePath::new("src/a.rs")],
            uses: uses
                .iter()
                .map(|(from, to)| (SymbolId::new(from), SymbolId::new(to)))
                .collect(),
        }
    }

    #[test]
    fn an_index_comes_back_and_a_new_one_replaces_it() {
        let folder = scratch("indexes");
        let path = folder.join("cyclops.db");
        let mut store = SqliteFactStore::open(&path).expect("opened");
        assert_eq!(store.load_indexes().expect("loaded"), Vec::new());
        let first = stored("Rust", &[("a::f", "a::g")]);
        let other = stored("TypeScript", &[]);
        store.save_index(&first).expect("saved");
        store.save_index(&other).expect("saved");
        assert_eq!(
            store.load_indexes().expect("loaded"),
            vec![first, other.clone()]
        );
        let again = stored("Rust", &[("a::f", "a::h")]);
        store.save_index(&again).expect("saved");
        let known = store.load_indexes().expect("loaded");
        fs::remove_dir_all(&folder).expect("removed");
        assert_eq!(known, vec![again, other]);
    }
}
