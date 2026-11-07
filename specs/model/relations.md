# Relations

Back to the [specification index](../README.md).

Relations describe how [symbols](symbols.md) connect. Each one has a kind, and a
confidence that says how much it can be trusted.

| Kind         | Meaning                                 |
| ------------ | --------------------------------------- |
| `imports`    | A module depends on another module.     |
| `implements` | A type implements an interface.         |
| `inherits`   | A class extends a class.                |
| `composes`   | A type has a field of another type.     |
| `uses`       | A function or method mentions a type.   |
| `calls`      | A function or method calls another one. |

| Confidence  | Meaning                                                                                    |
| ----------- | ------------------------------------------------------------------------------------------ |
| `exact`     | Written out in the code, or proven by [precise analysis](../analysis/precise-analysis.md). |
| `inferred`  | Matched by name, and only one symbol fits.                                                 |
| `ambiguous` | Matched by name, and several symbols fit. There is one relation for each.                  |

`from` and `to` name two symbols. A `relation?` or `confidence?` of `none` means there
is no such relation. A package
containing a module is the [hierarchy](symbols.md#the-hierarchy), not a relation.

## Structure

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation         | workspace | from                        | to                     | relation?  | confidence? |
| ------------------- | --------- | --------------------------- | ---------------------- | ---------- | ----------- |
| An import           | rust-shop | shop::orders                | shop::billing          | imports    | exact       |
| No import           | rust-shop | shop::notes                 | shop::billing          | none       | none        |
| A declaration       | rust-shop | shop                        | shop::orders           | none       | none        |
| A trait impl        | rust-shop | shop::catalog::Item         | shop::catalog::Priced  | implements | exact       |
| A field             | rust-shop | shop::billing::Invoice      | shop::catalog::Item    | composes   | exact       |
| A field in a `Vec`  | rust-shop | shop::orders::Order         | shop::billing::Invoice | composes   | exact       |
| A parameter         | rust-shop | shop::orders::Order::add    | shop::catalog::Item    | uses       | exact       |
| An import           | ts-shop   | src/orders                  | src/billing            | imports    | exact       |
| `implements`        | ts-shop   | src/catalog::Item           | src/catalog::Priced    | implements | exact       |
| A field             | ts-shop   | src/billing::Invoice        | src/catalog::Item      | composes   | exact       |
| A field in an array | ts-shop   | src/orders::Order           | src/billing::Invoice   | composes   | exact       |
| A parameter         | ts-shop   | src/orders::Order::add      | src/catalog::Item      | uses       | exact       |
| An ES import        | js-shop   | src/billing                 | src/catalog            | imports    | exact       |
| A `require`         | js-shop   | src/orders                  | src/billing            | imports    | exact       |
| `extends`           | js-shop   | src/billing::DiscountedItem | src/catalog::Item      | inherits   | exact       |

## Calls

The syntax layer knows the name of a called method, but not always the type it
belongs to. `self` and `this` are always known. For anything else, Cyclops looks for
methods with that name.

[//]: # "decisionTable Core"

| # situation         | workspace | from                            | to                              | relation? | confidence? |
| ------------------- | --------- | ------------------------------- | ------------------------------- | --------- | ----------- |
| On `self`           | rust-shop | shop::billing::Invoice::is_free | shop::billing::Invoice::total   | calls     | exact       |
| One candidate       | rust-shop | shop::orders::Order::total      | shop::billing::Invoice::is_free | calls     | inferred    |
| Two candidates      | rust-shop | shop::orders::Order::total      | shop::billing::Invoice::total   | calls     | ambiguous   |
| The other candidate | rust-shop | shop::orders::Order::total      | shop::orders::Order::total      | calls     | ambiguous   |
| On `this`           | ts-shop   | src/billing::Invoice::isFree    | src/billing::Invoice::total     | calls     | exact       |
| One candidate       | ts-shop   | src/orders::Order::total        | src/billing::Invoice::isFree    | calls     | inferred    |
| Two candidates      | ts-shop   | src/orders::Order::total        | src/billing::Invoice::total     | calls     | ambiguous   |

## Across files

A type's `impl` blocks count wherever they are. Imports point to the module that
really defines something, so a TypeScript barrel file does not hide it, and path
aliases from `tsconfig.json` are followed.

[//]: # "decisionTable Core"

| # situation       | workspace     | from                | to                     | relation? | confidence? |
| ----------------- | ------------- | ------------------- | ---------------------- | --------- | ----------- |
| An alias          | ts-resolution | src/app             | src/lib/helper         | imports   | exact       |
| Through a barrel  | ts-resolution | src/app             | src/barrel/thing       | imports   | exact       |
| The barrel itself | ts-resolution | src/app             | src/barrel             | none      | none        |
| A Rust re-export  | rust-reexport | reexport::app::make | reexport::inner::Thing | uses      | inferred    |
