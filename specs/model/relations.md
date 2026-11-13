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

### Python, Gleam and Elixir

A Python import may be relative, or start from the project's root or its `src` folder.
A base class that is an interface is implemented, and any other is inherited. Type
annotations give `composes` and `uses`.

Gleam imports name the module. A record field gives `composes`, and a parameter, or
building a record, gives `uses`.

An Elixir module depends on every module it names in an `alias`, `import`, `require`,
`use` or `@behaviour`, in a call, or in a struct such as `%Item{}`, so `alias Shop.{Invoice, Item}`
names both. A protocol's implementation implements it. A function that names a struct
uses it. Struct fields have no types, so Elixir has no `composes`.

Names from outside the project, such as the standard library, are left out.

[//]: # "decisionTable Core"

| # situation         | workspace  | from                            | to                          | relation?  | confidence? |
| ------------------- | ---------- | ------------------------------- | --------------------------- | ---------- | ----------- |
| An absolute import  | py-shop    | src/shop/orders                 | src/shop/billing            | imports    | exact       |
| A relative import   | py-shop    | src/shop/orders                 | src/shop/catalog            | imports    | exact       |
| No import           | py-shop    | src/shop/notes                  | src/shop/billing            | none       | none        |
| An abstract base    | py-shop    | src/shop/catalog::Item          | src/shop/catalog::Priced    | implements | exact       |
| A plain base        | py-shop    | src/shop/billing::DiscountedItem | src/shop/catalog::Item     | inherits   | exact       |
| An annotated field  | py-shop    | src/shop/billing::Invoice       | src/shop/catalog::Item      | composes   | exact       |
| A field in a list   | py-shop    | src/shop/orders::Order          | src/shop/billing::Invoice   | composes   | exact       |
| A parameter         | py-shop    | src/shop/orders::Order::add     | src/shop/catalog::Item      | uses       | exact       |
| A Gleam import      | gleam-shop | src/shop/orders                 | src/shop/billing            | imports    | exact       |
| Another one         | gleam-shop | src/shop/billing                | src/shop/catalog            | imports    | exact       |
| No import           | gleam-shop | src/shop/notes                  | src/shop/billing            | none       | none        |
| A record field      | gleam-shop | src/shop/billing::Invoice       | src/shop/catalog::Item      | composes   | exact       |
| A field in a list   | gleam-shop | src/shop/orders::Order          | src/shop/billing::Invoice   | composes   | exact       |
| A parameter         | gleam-shop | src/shop/orders::add            | src/shop/catalog::Item      | uses       | exact       |
| Building a record   | gleam-shop | src/shop/orders::add            | src/shop/billing::Invoice   | uses       | exact       |
| An `alias`          | ex-shop    | Shop.Order                      | Shop.Invoice                | imports    | exact       |
| One of an alias set | ex-shop    | Shop.Order                      | Shop.Item                   | imports    | exact       |
| A call by full name | ex-shop    | Shop.Invoice                    | Shop.Priced                 | imports    | exact       |
| An implementation   | ex-shop    | Shop.Item                       | Shop.Priced                 | imports    | exact       |
| No dependency       | ex-shop    | Shop.Notes                      | Shop.Invoice                | none       | none        |
| A protocol          | ex-shop    | Shop.Item::Item                 | Shop.Priced::Priced         | implements | exact       |
| A struct pattern    | ex-shop    | Shop.Order::add/2               | Shop.Item::Item             | uses       | exact       |

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

In Gleam and Elixir a call names its module, or the function is imported or local, so
the syntax layer knows what it calls. An Elixir call is matched on its arity too, and a
pipe passes one more argument. Python follows Rust and TypeScript: `self` is known, and
anything else is matched by name.

[//]: # "decisionTable Core"

| # situation         | workspace  | from                              | to                                | relation? | confidence? |
| ------------------- | ---------- | --------------------------------- | --------------------------------- | --------- | ----------- |
| On `self`           | py-shop    | src/shop/billing::Invoice::is_free | src/shop/billing::Invoice::total | calls     | exact       |
| One candidate       | py-shop    | src/shop/orders::Order::total     | src/shop/billing::Invoice::is_free | calls    | inferred    |
| Two candidates      | py-shop    | src/shop/orders::Order::total     | src/shop/billing::Invoice::total  | calls     | ambiguous   |
| Through its module  | gleam-shop | src/shop/orders::total            | src/shop/billing::total           | calls     | exact       |
| In its own module   | gleam-shop | src/shop/billing::is_free         | src/shop/billing::total           | calls     | exact       |
| Not the other one   | gleam-shop | src/shop/orders::total            | src/shop/orders::total            | none      | none        |
| Through an alias    | ex-shop    | Shop.Order::total/1               | Shop.Invoice::total/1             | calls     | exact       |
| A pipe              | ex-shop    | Shop.Order::add/2                 | Shop.Invoice::new/2               | calls     | exact       |
| In its own module   | ex-shop    | Shop.Invoice::free?/1             | Shop.Invoice::total/1             | calls     | exact       |
| A protocol function | ex-shop    | Shop.Invoice::total/1             | Shop.Priced::Priced::price/1      | calls     | exact       |

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
