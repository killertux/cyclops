# Hotspots

Back to the [specification index](../README.md).

A hotspot is code that is both complex and changed often. It is usually the best place
to start refactoring, since complex code nobody touches costs little.

| Metric    | Meaning                                                                         |
| --------- | ------------------------------------------------------------------------------- |
| `churn`   | How many commits changed the file within `metrics.hotspots.since`.              |
| `hotspot` | Its churn and complexity, each as a share of the project's highest, multiplied. |

The complexity of a file is the sum of the [cyclomatic complexity](cyclomatic-complexity.md)
of its functions. A hotspot of 1.00 is the worst the project has on both counts.

In `rust-shop`, `src/orders.rs` has a complexity of 4, `src/billing.rs` has 3, and
`src/catalog.rs` has 1. `git init` is the first commit of every file.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation       | workspace | steps                                                                                                  | subject        | churn? | hotspot? |
| ----------------- | --------- | ------------------------------------------------------------------------------------------------------ | -------------- | ------ | -------- |
| The worst of both | rust-shop | `git init; commit src/orders.rs 9 times; commit src/billing.rs 7 times; commit src/catalog.rs 1 times` | src/orders.rs  | 10     | 1.00     |
| Close behind      | rust-shop | `git init; commit src/orders.rs 9 times; commit src/billing.rs 7 times; commit src/catalog.rs 1 times` | src/billing.rs | 8      | 0.60     |
| Simple code       | rust-shop | `git init; commit src/orders.rs 9 times; commit src/billing.rs 7 times; commit src/catalog.rs 1 times` | src/catalog.rs | 2      | 0.05     |
| A module          | rust-shop | `git init; commit src/orders.rs 9 times; commit src/billing.rs 7 times; commit src/catalog.rs 1 times` | shop::billing  | 8      | 0.60     |

## How far back

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation     | workspace | steps                                                                                        | subject       | churn? |
| --------------- | --------- | -------------------------------------------------------------------------------------------- | ------------- | ------ |
| Old commits     | rust-shop | `git init; commit src/orders.rs 4 times 13 months ago`                                       | src/orders.rs | 1      |
| A longer window | rust-shop | `set metrics.hotspots.since 24 months; git init; commit src/orders.rs 4 times 13 months ago` | src/orders.rs | 5      |

## Renamed files

A file keeps its history when it is renamed.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation | workspace | steps                                                                           | subject          | churn? |
| ----------- | --------- | ------------------------------------------------------------------------------- | ---------------- | ------ |
| Renamed     | ts-shop   | `git init; commit src/orders.ts 2 times; rename src/orders.ts src/purchases.ts` | src/purchases.ts | 4      |

## Test code

[Test code](../analysis/test-code.md#leaving-it-out) that is left out has no hotspot.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation | workspace | steps                                      | subject         | churn? | hotspot? |
| ----------- | --------- | ------------------------------------------ | --------------- | ------ | -------- |
| Left out    | rust-shop | `git init; commit tests/orders.rs 3 times` | tests/orders.rs | n/a    | n/a      |

## Without git

Hotspots need history. Without it the metric is unavailable, and everything else
still works. `unavailable?` lists the metrics that could not be worked out, and why.
This is a normal state, so it is not listed in `problems?`.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation | workspace | subject       | churn? | hotspot? | cyclomatic? | unavailable?                   |
| ----------- | --------- | ------------- | ------ | -------- | ----------- | ------------------------------ |
| No git      | rust-shop | src/orders.rs | n/a    | n/a      | 4           | churn, hotspot: no git history |
