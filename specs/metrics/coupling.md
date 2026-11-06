# Coupling

Back to the [specification index](../README.md).

Coupling measures how modules depend on each other, using Robert C. Martin's package
metrics.

| Metric         | Meaning                                                                        |
| -------------- | ------------------------------------------------------------------------------ |
| `afferent`     | How many modules depend on this one.                                           |
| `efferent`     | How many modules this one depends on.                                          |
| `instability`  | efferent / (afferent + efferent). 0 is stable, 1 is unstable.                  |
| `abstractness` | Interfaces / all types of the module.                                          |
| `distance`     | How far it is from the balance of the two: \|abstractness + instability - 1\|. |

## Instability

In all three shop projects, `orders` depends on `billing` and `catalog`, and `billing`
depends on `catalog`. A module with no dependencies either way has no instability.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation         | workspace | subject       | afferent? | efferent? | instability? |
| ------------------- | --------- | ------------- | --------- | --------- | ------------ |
| Depends on others   | rust-shop | shop::orders  | 0         | 2         | 1.00         |
| In the middle       | rust-shop | shop::billing | 1         | 1         | 0.50         |
| Others depend on it | rust-shop | shop::catalog | 2         | 0         | 0.00         |
| Alone               | rust-shop | shop::notes   | 0         | 0         | n/a          |
| Depends on others   | ts-shop   | src/orders    | 0         | 2         | 1.00         |
| In the middle       | ts-shop   | src/billing   | 1         | 1         | 0.50         |
| Others depend on it | ts-shop   | src/catalog   | 2         | 0         | 0.00         |
| Alone               | ts-shop   | src/notes     | 0         | 0         | n/a          |
| Depends on others   | js-shop   | src/orders    | 0         | 2         | 1.00         |
| In the middle       | js-shop   | src/billing   | 1         | 1         | 0.50         |
| Others depend on it | js-shop   | src/catalog   | 2         | 0         | 0.00         |
| Alone               | js-shop   | src/notes     | 0         | 0         | n/a          |

A module counts once, however many things are imported from it. `src/billing.rs` takes
two names from `catalog` and counts it once, as the second row shows. Dependencies
of [test code](../analysis/test-code.md#leaving-it-out) are left out with it.

## Abstractness and distance

A module with no types has neither.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation     | workspace | subject       | abstractness? | distance? |
| --------------- | --------- | ------------- | ------------- | --------- |
| Half interfaces | rust-shop | shop::catalog | 0.50          | 0.50      |
| No interfaces   | rust-shop | shop::billing | 0.00          | 0.50      |
| On the balance  | rust-shop | shop::orders  | 0.00          | 0.00      |
| No types        | rust-shop | shop::notes   | n/a           | n/a       |
| Half interfaces | ts-shop   | src/catalog   | 0.50          | 0.50      |
| On the balance  | ts-shop   | src/orders    | 0.00          | 0.00      |
| No interfaces   | js-shop   | src/catalog   | 0.00          | 1.00      |
| No types        | js-shop   | src/orders    | n/a           | n/a       |
