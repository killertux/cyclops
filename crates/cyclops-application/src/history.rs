//! Churn and hotspots, from the history of the project's files. See
//! `specs/metrics/hotspots.md`.

use crate::analysis::AnalysisInput;
use crate::ports::Commit;
use cyclops_domain::metric::{Measurement, MetricId, Subject};
use cyclops_domain::path::RelativePath;
use cyclops_domain::settings::{Period, PeriodUnit};
use cyclops_domain::symbol::SymbolKind;
use std::collections::{BTreeMap, HashMap, HashSet};

const DAY: i64 = 24 * 60 * 60;

/// When a period back from `now` starts, in seconds since 1970. A month is 30 days.
#[must_use]
pub fn start_of(period: Period, now: i64) -> i64 {
    let days = match period.unit {
        PeriodUnit::Days => 1,
        PeriodUnit::Weeks => 7,
        PeriodUnit::Months => 30,
        PeriodUnit::Years => 365,
    };
    now.saturating_sub(
        i64::from(period.amount)
            .saturating_mul(days)
            .saturating_mul(DAY),
    )
}

/// A commit as an explanation shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    /// The first seven characters of its id.
    pub short_id: String,
    pub time: i64,
    pub subject: String,
}

/// Calls `found` once for each file a commit since `since` changed, newest first. A
/// renamed file keeps the history of its old name.
fn walk(
    commits: &[Commit],
    current: &[&RelativePath],
    since: i64,
    mut found: impl FnMut(&RelativePath, &Commit),
) {
    // What each name stood for at the point the walk has reached, newest first.
    let mut names: HashMap<String, RelativePath> = current
        .iter()
        .map(|path| (path.as_str().to_owned(), (*path).clone()))
        .collect();
    for commit in commits {
        let mut counted: HashSet<RelativePath> = HashSet::new();
        let mut renames = Vec::new();
        for change in &commit.changes {
            let Some(file) = names.get(&change.path).cloned() else {
                continue;
            };
            if commit.time >= since && counted.insert(file.clone()) {
                found(&file, commit);
            }
            if let Some(previous) = &change.previous {
                renames.push((change.path.clone(), previous.clone(), file));
            }
        }
        for (path, previous, file) in renames {
            names.remove(&path);
            names.insert(previous, file);
        }
    }
}

/// How many commits since `since` changed each current file.
#[must_use]
pub fn churn(
    commits: &[Commit],
    current: &[&RelativePath],
    since: i64,
) -> BTreeMap<RelativePath, u32> {
    let mut counts: BTreeMap<RelativePath, u32> = BTreeMap::new();
    walk(commits, current, since, |file, _| {
        *counts.entry(file.clone()).or_insert(0) += 1;
    });
    counts
}

/// The commits since `since` that changed each current file, newest first.
#[must_use]
pub fn file_commits(
    commits: &[Commit],
    current: &[&RelativePath],
    since: i64,
) -> BTreeMap<RelativePath, Vec<CommitSummary>> {
    let mut found: BTreeMap<RelativePath, Vec<CommitSummary>> = BTreeMap::new();
    walk(commits, current, since, |file, commit| {
        found.entry(file.clone()).or_default().push(CommitSummary {
            short_id: commit.id.chars().take(7).collect(),
            time: commit.time,
            subject: commit.subject.clone(),
        });
    });
    found
}

fn share(value: f64, highest: f64) -> f64 {
    if highest > 0.0 { value / highest } else { 0.0 }
}

/// Churn and hotspots for every file that counts, and the highest of them for every
/// module and package.
#[must_use]
pub fn measure(
    input: &AnalysisInput<'_>,
    churn: &BTreeMap<RelativePath, u32>,
    complexity: &BTreeMap<RelativePath, f64>,
) -> Vec<Measurement> {
    let files: Vec<_> = input.counted_files().collect();
    let churn_of = |path: &RelativePath| f64::from(churn.get(path).copied().unwrap_or(0));
    let complexity_of = |path: &RelativePath| complexity.get(path).copied().unwrap_or(0.0);
    let most_churn = files
        .iter()
        .map(|file| churn_of(&file.path))
        .fold(0.0, f64::max);
    let most_complexity = files
        .iter()
        .map(|file| complexity_of(&file.path))
        .fold(0.0, f64::max);

    let mut highest: BTreeMap<(Subject, MetricId), f64> = BTreeMap::new();
    let mut measurements = Vec::new();
    for file in files {
        let churn = churn_of(&file.path);
        let hotspot = share(churn, most_churn) * share(complexity_of(&file.path), most_complexity);
        for (metric, value) in [(MetricId::Churn, churn), (MetricId::Hotspot, hotspot)] {
            measurements.push(Measurement {
                metric,
                subject: Subject::File(file.path.clone()),
                value,
            });
            let mut current = file.module.clone();
            while let Some(id) = current.take() {
                let Some(symbol) = input.model.symbol(&id) else {
                    break;
                };
                if matches!(
                    symbol.kind,
                    SymbolKind::Module | SymbolKind::Package | SymbolKind::Project
                ) {
                    let slot = highest
                        .entry((Subject::Symbol(id.clone()), metric))
                        .or_insert(0.0);
                    *slot = slot.max(value);
                }
                current.clone_from(&symbol.parent);
            }
        }
    }
    measurements.extend(
        highest
            .into_iter()
            .map(|((subject, metric), value)| Measurement {
                metric,
                subject,
                value,
            }),
    );
    measurements
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FileChange;

    fn commit(time: i64, changes: &[(&str, Option<&str>)]) -> Commit {
        Commit {
            id: time.to_string(),
            time,
            subject: String::new(),
            changes: changes
                .iter()
                .map(|(path, previous)| FileChange {
                    path: (*path).to_owned(),
                    previous: previous.map(ToOwned::to_owned),
                })
                .collect(),
        }
    }

    #[test]
    fn a_renamed_file_keeps_its_history() {
        let commits = [
            commit(4, &[("b.ts", Some("a.ts"))]),
            commit(3, &[("a.ts", None)]),
            commit(2, &[("a.ts", None), ("c.ts", None)]),
        ];
        let current = RelativePath::new("b.ts");
        let counts = churn(&commits, &[&current], 0);
        assert_eq!(counts.get(&current), Some(&3));
    }

    #[test]
    fn old_commits_do_not_count() {
        let commits = [
            commit(100, &[("a.ts", None)]),
            commit(10, &[("a.ts", None)]),
        ];
        let current = RelativePath::new("a.ts");
        assert_eq!(churn(&commits, &[&current], 50).get(&current), Some(&1));
    }

    #[test]
    fn a_month_is_thirty_days() {
        let period = Period {
            amount: 1,
            unit: PeriodUnit::Months,
        };
        assert_eq!(start_of(period, 30 * DAY), 0);
    }
}
