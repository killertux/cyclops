# Impact

Back to the [specification index](../README.md).

Impact answers "what would changing this affect?". It follows [relations](../model/relations.md)
backwards from a symbol, to what depends on it. A [diagram](diagrams.md) follows both
directions, so it cannot answer this alone.

`dependents?` lists the symbols exactly `distance` relations away. A symbol is only
listed at its nearest distance, and the subject never depends on itself here.

## Modules

The cycles samples have `a` and `b` importing each other, and `f` importing `a`.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation         | workspace   | subject       | distance | dependents?                 |
| ------------------- | ----------- | ------------- | -------- | --------------------------- |
| Direct              | rust-shop   | shop::catalog | 1        | shop::billing, shop::orders |
| Nothing further     | rust-shop   | shop::catalog | 2        | none                        |
| One dependent       | rust-shop   | shop::billing | 1        | shop::orders                |
| Depended on by none | rust-shop   | shop::orders  | 1        | none                        |
| No relations        | rust-shop   | shop::notes   | 1        | none                        |
| Through a cycle     | rust-cycles | cycles::b     | 1        | cycles::a                   |
| Further through     | rust-cycles | cycles::b     | 2        | cycles::f                   |
| Never itself        | rust-cycles | cycles::a     | 2        | none                        |

## Members

The subject's level decides which relations are followed. A function follows `calls`,
and a type follows `uses`, `composes`, `implements` and `inherits`.

[//]: # "decisionTable Core"

| # situation   | workspace | subject                       | distance | dependents?                                                 |
| ------------- | --------- | ----------------------------- | -------- | ----------------------------------------------------------- |
| Callers       | rust-shop | shop::billing::Invoice::total | 1        | shop::billing::Invoice::is_free, shop::orders::Order::total |
| Their callers | rust-shop | shop::billing::Invoice::total | 2        | none                                                        |
| A used type   | rust-shop | shop::catalog::Item           | 1        | shop::billing::Invoice, shop::orders::Order::add            |
| A base class  | js-shop   | src/catalog::Item             | 1        | src/billing::DiscountedItem, src/orders::total              |

## How sure

Impact includes guesses, and says how many. `exact?`, `inferred?` and `ambiguous?` count
the dependents at every distance. A chain is only as sure as its weakest relation, so
each dependent counts by the weakest relation on its way to the subject. When several
ways lead there, the surest one counts. In [`rust-chain`](../../samples/rust-chain), `drive`
makes a guessed call to `start`, and `race` calls `drive` exactly.

[//]: # "decisionTable Core"

| # situation     | workspace  | steps              | subject                         | exact? | inferred? | ambiguous? |
| --------------- | ---------- | ------------------ | ------------------------------- | ------ | --------- | ---------- |
| Proven imports  | rust-shop  |                    | shop::catalog                   | 2      | 0         | 0          |
| A name match    | rust-shop  |                    | shop::billing::Invoice::is_free | 0      | 1         | 0          |
| Several fit     | rust-shop  |                    | shop::billing::Invoice::total   | 1      | 0         | 1          |
| After precise   | rust-shop  | `precise analysis` | shop::billing::Invoice::total   | 2      | 0         | 0          |
| Through a guess | rust-chain |                    | chain::Engine::start            | 0      | 2         | 0          |

## Reach

`reach?` counts every dependent at any distance, and `packages?` counts the packages
they are in.

[//]: # "decisionTable Core"

| # situation | workspace   | subject       | reach? | packages? |
| ----------- | ----------- | ------------- | ------ | --------- |
| Two modules | rust-shop   | shop::catalog | 2      | 1         |
| Nothing     | rust-shop   | shop::notes   | 0      | 0         |
| A cycle     | rust-cycles | cycles::b     | 2      | 1         |

## Test code

[Test code](../analysis/test-code.md#leaving-it-out) that is left out depends on nothing.

[//]: # "decisionTable Core"

| # situation | workspace | steps                    | subject      | distance | dependents?  |
| ----------- | --------- | ------------------------ | ------------ | -------- | ------------ |
| Left out    | rust-shop |                          | shop::orders | 1        | none         |
| Included    | rust-shop | `set tests.include true` | shop::orders | 1        | tests/orders |
