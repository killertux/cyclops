//! The log, in `.cyclops/logs/cyclops.log`: what each analysis did, to read when
//! something goes wrong. Writing it never stops an analysis.

use cyclops_application::ports::Log;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the log is moved aside, and a new one starts.
const LIMIT: u64 = 10 * 1024 * 1024;

/// A log that grows at the end of a file.
pub struct FileLog {
    path: PathBuf,
    /// Why it could not be written, the first time it could not. Holding it also keeps
    /// entries from two threads apart.
    trouble: Mutex<Option<String>>,
}

impl FileLog {
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            trouble: Mutex::new(None),
        }
    }

    fn append(&self, text: &str) -> io::Result<()> {
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder)?;
        }
        if fs::metadata(&self.path).is_ok_and(|found| found.len() > LIMIT) {
            fs::rename(&self.path, self.path.with_extension("log.1"))?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(text.as_bytes())
    }
}

impl Log for FileLog {
    fn write(&self, entry: &str) {
        let text = entry_text(now(), entry);
        let mut trouble = self
            .trouble
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Err(error) = self.append(&text) {
            trouble.get_or_insert_with(|| format!("{}: {error}", self.path.display()));
        }
    }

    fn trouble(&self) -> Option<String> {
        self.trouble
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// The seconds since 1970, now.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// An entry as the file has it: its first line after the time, and the rest indented.
fn entry_text(seconds: u64, entry: &str) -> String {
    let mut lines = entry.lines();
    let mut text = format!(
        "{} {}\n",
        timestamp(seconds),
        lines.next().unwrap_or_default()
    );
    for line in lines {
        text.push_str("    ");
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// A time as `2026-11-12 11:28:03`, in UTC.
fn timestamp(seconds: u64) -> String {
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The date of a day counted from 1970-01-01, by Howard Hinnant's `civil_from_days`.
const fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_written_as_dates_in_utc() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00");
        assert_eq!(timestamp(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(timestamp(1_794_483_683), "2026-11-12 11:41:23");
    }

    #[test]
    fn the_lines_after_the_first_are_indented() {
        assert_eq!(
            entry_text(0, "rust-analyzer ended\nError: no projects"),
            "1970-01-01 00:00:00 rust-analyzer ended\n    Error: no projects\n"
        );
    }

    #[test]
    fn a_large_log_is_moved_aside() {
        let folder = std::env::temp_dir().join(format!("cyclops-log-{}", std::process::id()));
        let path = folder.join("cyclops.log");
        fs::create_dir_all(&folder).unwrap();
        fs::write(&path, vec![b'x'; usize::try_from(LIMIT + 1).unwrap()]).unwrap();
        let log = FileLog::new(&path);
        log.write("analysis started");
        let kept = fs::read_to_string(&path).unwrap();
        assert!(kept.ends_with("analysis started\n"));
        assert!(kept.len() < 100);
        assert!(folder.join("cyclops.log.1").exists());
        assert_eq!(log.trouble(), None);
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_log_that_cannot_be_written_says_why() {
        let blocker = std::env::temp_dir().join(format!("cyclops-log-file-{}", std::process::id()));
        fs::write(&blocker, "a file, not a folder").unwrap();
        let log = FileLog::new(&blocker.join("cyclops.log"));
        log.write("analysis started");
        assert!(log.trouble().is_some());
        fs::remove_file(&blocker).unwrap();
    }
}
