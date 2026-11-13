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
[//]: # "decisionTable Core"

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

The Python and Gleam shops share that graph. Elixir names its modules after what they
hold, so `ex-shop` has `Shop.Order`, `Shop.Invoice` and `Shop.Item`, and `Shop.Priced`
is the protocol that items implement and invoices call. `Shop.Order` depends on
`Shop.Invoice` and `Shop.Item`, `Shop.Invoice` on `Shop.Item` and `Shop.Priced`, and
`Shop.Item` on `Shop.Priced`.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation         | workspace  | subject          | afferent? | efferent? | instability? |
| ------------------- | ---------- | ---------------- | --------- | --------- | ------------ |
| Depends on others   | py-shop    | src/shop/orders  | 0         | 2         | 1.00         |
| In the middle       | py-shop    | src/shop/billing | 1         | 1         | 0.50         |
| Others depend on it | py-shop    | src/shop/catalog | 2         | 0         | 0.00         |
| Alone               | py-shop    | src/shop/notes   | 0         | 0         | n/a          |
| Depends on others   | gleam-shop | src/shop/orders  | 0         | 2         | 1.00         |
| In the middle       | gleam-shop | src/shop/billing | 1         | 1         | 0.50         |
| Others depend on it | gleam-shop | src/shop/catalog | 2         | 0         | 0.00         |
| Alone               | gleam-shop | src/shop/notes   | 0         | 0         | n/a          |
| Depends on others   | ex-shop    | Shop.Order       | 0         | 2         | 1.00         |
| In the middle       | ex-shop    | Shop.Invoice     | 1         | 2         | 0.67         |
| Lower down          | ex-shop    | Shop.Item        | 2         | 1         | 0.33         |
| Others depend on it | ex-shop    | Shop.Priced      | 2         | 0         | 0.00         |
| Alone               | ex-shop    | Shop.Notes       | 0         | 0         | n/a          |

A module counts once, however many things are imported from it. `src/billing.rs` takes
two names from `catalog` and counts it once, as the second row shows. Dependencies
of [test code](../analysis/test-code.md#leaving-it-out) are left out with it.

## Abstractness and distance

A module with no types has neither.

[//]: # "decisionTable Core"

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

Gleam has no interfaces, so a Gleam module is never abstract. An Elixir protocol is an
interface.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation     | workspace  | subject          | abstractness? | distance? |
| --------------- | ---------- | ---------------- | ------------- | --------- |
| Half interfaces | py-shop    | src/shop/catalog | 0.50          | 0.50      |
| On the balance  | py-shop    | src/shop/orders  | 0.00          | 0.00      |
| No types        | py-shop    | src/shop/notes   | n/a           | n/a       |
| No interfaces   | gleam-shop | src/shop/catalog | 0.00          | 1.00      |
| On the balance  | gleam-shop | src/shop/orders  | 0.00          | 0.00      |
| A protocol      | ex-shop    | Shop.Priced      | 1.00          | 0.00      |
| A struct        | ex-shop    | Shop.Item        | 0.00          | 0.67      |
| No types        | ex-shop    | Shop.Notes       | n/a           | n/a       |
