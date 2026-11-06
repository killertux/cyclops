# Ranking

Back to the [specification index](../README.md).

A number means little alone. Ranking says where a symbol stands among its peers, one
metric at a time. It never combines metrics, because Cyclops has no health score.

The peers of a symbol are the symbols of the same level that have a value for the
metric: functions with functions, types with types, modules with modules and files
with files. Rank 1 is the worst, so it depends on the metric's
[direction](../metrics/catalog.md#direction). A neutral metric has no worst, so its rank 1
is simply its highest value. Symbols that tie share a rank, and the next rank is skipped.

`rank?` is the position and `of?` is how many peers there are.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation    | workspace | subject                       | metric      | rank? | of? |
| -------------- | --------- | ----------------------------- | ----------- | ----- | --- |
| The worst      | rust-shop | shop::orders::Order::total    | cyclomatic  | 1     | 6   |
| Second         | rust-shop | shop::billing::Invoice::total | cyclomatic  | 2     | 6   |
| A tie          | rust-shop | shop::notes::greeting         | cyclomatic  | 3     | 6   |
| A tie, too     | rust-shop | shop::orders::Order::add      | cyclomatic  | 3     | 6   |
| A module       | rust-shop | shop::orders                  | instability | 1     | 3   |
| The lowest     | rust-shop | shop::catalog                 | instability | 3     | 3   |
| A rollup       | rust-shop | shop::orders                  | cyclomatic  | 1     | 4   |
| No value       | rust-shop | shop::notes                   | instability | n/a   | n/a |
| Does not apply | rust-shop | shop::orders                  | lcom        | n/a   | n/a |

## Spread

`spread?` shows how a metric is distributed across the peers. It is eight equal ranges
from the lowest value to the highest, with a count for each.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation | workspace | level    | metric      | spread?                |
| ----------- | --------- | -------- | ----------- | ---------------------- |
| Functions   | rust-shop | function | cyclomatic  | 4, 0, 0, 0, 1, 0, 0, 1 |
| Modules     | rust-shop | module   | instability | 1, 0, 0, 0, 1, 0, 0, 1 |

## Worst per metric

`worst?` lists the symbols ranked first for a metric, in alphabetical order. It names up
to three, then says how many more tie with them. `worst` looks at the level where the
metric is measured, not at rollups.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation  | workspace       | metric      | worst?                                                       | value? |
| ------------ | --------------- | ----------- | ------------------------------------------------------------ | ------ |
| A function   | rust-shop       | cyclomatic  | shop::orders::Order::total                                   | 3      |
| A module     | rust-shop       | instability | shop::orders                                                 | 1.00   |
| A type       | rust-split-impl | lcom        | split::store::Store                                          | 2      |
| A tie        | rust-cycles     | cycle_size  | cycles::c, cycles::d, cycles::e                              | 3      |
| A larger tie | rust-cycles     | cyclomatic  | cycles::a::link, cycles::b::link, cycles::c::link and 3 more | 1      |
| Nothing      | docs-only       | cyclomatic  | none                                                         | n/a    |
