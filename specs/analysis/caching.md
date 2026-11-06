# Caching

Back to the [specification index](../README.md).

On a large repository, a repeat analysis must only pay for what changed. The
`files parsed?` column counts the files parsed by the final analysis. `rust-shop`
has 6 source files, and `ts-shop` has 5.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation        | workspace | steps                                            | files parsed? |
| ------------------ | --------- | ------------------------------------------------ | ------------- |
| The first analysis | rust-shop |                                                  | 6             |
| Nothing changed    | rust-shop | `analyze`                                        | 0             |
| One file changed   | rust-shop | `analyze; edit src/orders.rs`                    | 1             |
| Two files changed  | rust-shop | `analyze; edit src/orders.rs; edit src/notes.rs` | 2             |
| Same contents      | rust-shop | `analyze; touch src/orders.rs`                   | 0             |
| A new file         | rust-shop | `analyze; add src/extra.rs`                      | 1             |
| A deleted file     | rust-shop | `analyze; delete src/notes.rs`                   | 0             |

## Changing settings

Thresholds and filters apply to what is already known, so they never re-parse.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation        | workspace | steps                                                      | files parsed? |
| ------------------ | --------- | ---------------------------------------------------------- | ------------- |
| A new alert rule   | rust-shop | `analyze; add alert cyclomatic function > 2 warning`       | 0             |
| Test code included | rust-shop | `analyze; set tests.include true`                          | 0             |
| Test code left out | rust-shop | `set tests.include true; analyze; set tests.include false` | 0             |
| A path excluded    | rust-shop | `analyze; set project.exclude src/notes.rs`                | 0             |

## Rebuilding the cache

The cache is disposable. When it is missing, damaged or out of date, Cyclops rebuilds
it, and that is not a problem to report.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation         | workspace | steps                           | files parsed? | problems? |
| ------------------- | --------- | ------------------------------- | ------------- | --------- |
| Deleted             | rust-shop | `analyze; delete cache`         | 6             | none      |
| Corrupted           | rust-shop | `analyze; corrupt cache`        | 6             | none      |
| Older schema        | rust-shop | `analyze; outdate cache`        | 6             | none      |
| Its adapter changed | rust-shop | `analyze; upgrade rust adapter` | 6             | none      |
| Another adapter     | ts-shop   | `analyze; upgrade rust adapter` | 0             | none      |

## Same results

A cached result is the same as a fresh one.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation | workspace | steps                   | subject                    | cyclomatic? | lines? |
| ----------- | --------- | ----------------------- | -------------------------- | ----------- | ------ |
| Fresh       | rust-shop |                         | shop::orders::Order::total | 3           | 9      |
| From cache  | rust-shop | `analyze`               | shop::orders::Order::total | 3           | 9      |
| Rebuilt     | rust-shop | `analyze; delete cache` | shop::orders::Order::total | 3           | 9      |

## Git history

Only commits that are new since the last analysis are read. `commits read?` counts
them. If the known history was rewritten, all of it is read again.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation       | workspace | steps                                                              | commits read? |
| ----------------- | --------- | ------------------------------------------------------------------ | ------------- |
| The first read    | rust-shop | `git init; commit src/orders.rs 3 times`                           | 4             |
| Nothing new       | rust-shop | `git init; commit src/orders.rs 3 times; analyze`                  | 0             |
| New commits       | rust-shop | `git init; analyze; commit src/orders.rs 2 times`                  | 2             |
| Rewritten history | rust-shop | `git init; commit src/orders.rs 3 times; analyze; rewrite history` | 4             |
