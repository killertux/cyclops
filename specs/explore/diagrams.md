# Diagrams

Back to the [specification index](../README.md).

A diagram shows the structure of a project with metrics attached. A whole large
project cannot be read at once, so a diagram usually starts from one symbol and shows
what is around it. This page says what a diagram contains, and each interface decides
how to draw it.

| Input       | Meaning                                                                        |
| ----------- | ------------------------------------------------------------------------------ |
| `level`     | `package`, `module`, `type` or `member`. Only symbols of that level are shown. |
| `focus`     | The symbol to start from. Empty means every symbol of the level.               |
| `depth`     | How many relations to follow from the focus. Both directions count.            |
| `relations` | The [relation kinds](../model/relations.md) to show. Empty means all.          |
| `overlay`   | The metric to attach to each node.                                             |
| `filter`    | Keeps only what is [kept](filters.md#in-a-hierarchy) by a filter.              |

`nodes?` lists what is shown. `edges?` lists the relations between shown nodes.

## Levels

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation   | workspace | level   | relations | nodes?                                                                                  | edges?                                                                                                                     |
| ------------- | --------- | ------- | --------- | --------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Every module  | rust-shop | module  |           | shop::billing, shop::catalog, shop::notes, shop::orders                                 | shop::billing -> shop::catalog (imports), shop::orders -> shop::billing (imports), shop::orders -> shop::catalog (imports) |
| Every package | rust-shop | package |           | shop                                                                                    | none                                                                                                                       |
| Every type    | rust-shop | type    | composes  | shop::billing::Invoice, shop::catalog::Item, shop::catalog::Priced, shop::orders::Order | shop::billing::Invoice -> shop::catalog::Item (composes), shop::orders::Order -> shop::billing::Invoice (composes)         |
| A TS project  | ts-shop   | module  |           | src/billing, src/catalog, src/notes, src/orders                                         | src/billing -> src/catalog (imports), src/orders -> src/billing (imports), src/orders -> src/catalog (imports)             |

A filter applies to what is inside a node too, so a module stays when one of its methods
matches.

[//]: # "decisionTable Core"

| # situation   | workspace | level  | filter                             | nodes?                      |
| ------------- | --------- | ------ | ---------------------------------- | --------------------------- |
| Inside a node | rust-shop | module | `kind = method and cyclomatic > 1` | shop::billing, shop::orders |

## Focusing

[//]: # "decisionTable Core"

| # situation       | workspace | level  | focus         | depth | nodes?                                     | edges?                                                                                                                     |
| ----------------- | --------- | ------ | ------------- | ----- | ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| Only the focus    | rust-shop | module | shop::orders  | 0     | shop::orders                               | none                                                                                                                       |
| Its neighbors     | rust-shop | module | shop::billing | 1     | shop::billing, shop::catalog, shop::orders | shop::billing -> shop::catalog (imports), shop::orders -> shop::billing (imports), shop::orders -> shop::catalog (imports) |
| Depended on       | rust-shop | module | shop::catalog | 1     | shop::billing, shop::catalog, shop::orders | shop::billing -> shop::catalog (imports), shop::orders -> shop::billing (imports), shop::orders -> shop::catalog (imports) |
| With no relations | rust-shop | module | shop::notes   | 1     | shop::notes                                | none                                                                                                                       |

## Drilling in

A focus of a higher level than `level` shows what is inside it.

[//]: # "decisionTable Core"

| # situation    | workspace | level  | focus               | depth | nodes?                                                                              |
| -------------- | --------- | ------ | ------------------- | ----- | ----------------------------------------------------------------------------------- |
| Into a package | rust-shop | module | shop                | 0     | shop::billing, shop::catalog, shop::notes, shop::orders                             |
| Into a module  | rust-shop | type   | shop::orders        | 0     | shop::orders::Order                                                                 |
| Into a type    | rust-shop | member | shop::orders::Order | 0     | shop::orders::Order::add, shop::orders::Order::invoices, shop::orders::Order::total |

## Metrics on a diagram

`overlay value?` reads the `overlay` metric on the `subject` node. Any metric can be
attached, and a [rollup](../metrics/rollups.md) makes it available on larger symbols.

[//]: # "decisionTable Core"

| # situation     | workspace | level  | overlay     | subject             | overlay value? |
| --------------- | --------- | ------ | ----------- | ------------------- | -------------- |
| A module metric | rust-shop | module | instability | shop::orders        | 1.00           |
| A rollup        | rust-shop | module | cyclomatic  | shop::orders        | 3              |
| A type metric   | rust-shop | type   | lcom        | shop::orders::Order | 1              |
| Does not apply  | rust-shop | module | lcom        | shop::orders        | n/a            |

## Marks

A node says whether it has an alert and whether it is in a cycle. An edge says how
confident it is, and whether it is part of a cycle.

[//]: # "decisionTable Core"

| # situation     | workspace   | steps                                       | level  | subject      | alerted? | in cycle? |
| --------------- | ----------- | ------------------------------------------- | ------ | ------------ | -------- | --------- |
| An alert        | rust-shop   | `add alert instability module >= 1 warning` | module | shop::orders | yes      | false     |
| A cycle         | rust-cycles |                                             | module | cycles::a    | no       | true      |
| Next to a cycle | rust-cycles |                                             | module | cycles::f    | no       | false     |

[//]: # "decisionTable Core"

| # situation  | workspace   | level  | from                       | to                            | edge confidence? | edge in cycle? |
| ------------ | ----------- | ------ | -------------------------- | ----------------------------- | ---------------- | -------------- |
| Exact        | rust-shop   | module | shop::orders               | shop::billing                 | exact            | no             |
| Guessed      | rust-shop   | member | shop::orders::Order::total | shop::billing::Invoice::total | ambiguous        | no             |
| In a cycle   | rust-cycles | module | cycles::a                  | cycles::b                     | exact            | yes            |
| Into a cycle | rust-cycles | module | cycles::f                  | cycles::a                     | exact            | no             |

## Class boxes

At `type` level a node also carries what a class box needs. `stereotype?` is `interface`,
`enum` or `alias`, and `none` for a struct or a class. `fields?` and `methods?` list its
members. A method of a trait or an interface counts as a method of it.

[//]: # "decisionTable Core"

| # situation  | workspace | level | subject                | stereotype? | fields?        | methods?       |
| ------------ | --------- | ----- | ---------------------- | ----------- | -------------- | -------------- |
| A struct     | rust-shop | type  | shop::orders::Order    | none        | invoices       | add, total     |
| Two of each  | rust-shop | type  | shop::billing::Invoice | none        | discount, item | is_free, total |
| An interface | rust-shop | type  | shop::catalog::Priced  | interface   | none           | price          |
| A class      | ts-shop   | type  | src/orders::Order      | none        | invoices       | add, total     |

[//]: # "decisionTable Snippet"

| # situation | language | code           | symbol | stereotype? |
| ----------- | -------- | -------------- | ------ | ----------- |
| An enum     | rust     | `enum E { A }` | E      | enum        |
| An alias    | rust     | `type A = u8;` | A      | alias       |
| A trait     | rust     | `trait T {}`   | T      | interface   |

`visibility?` says who can reach a member: `public`, `restricted` for anything between,
or `private`.

[//]: # "decisionTable Snippet"

| # situation | language   | code                            | symbol | visibility? |
| ----------- | ---------- | ------------------------------- | ------ | ----------- |
| Public      | rust       | `struct S { pub a: u8 }`        | S::a   | public      |
| Private     | rust       | `struct S { a: u8 }`            | S::a   | private     |
| Restricted  | rust       | `struct S { pub(crate) a: u8 }` | S::a   | restricted  |
| Public, TS  | typescript | `class C { x() {} }`            | C::x   | public      |
| Private, TS | typescript | `class C { private x() {} }`    | C::x   | private     |
| Protected   | typescript | `class C { protected x() {} }`  | C::x   | restricted  |

The overlay and the marks are on members too. `member value?` reads the `overlay` on the
`subject` member, and a member with no value shows `n/a`.

[//]: # "decisionTable Core"

| # situation | workspace | steps                                       | overlay    | subject                       | member value? | alerted? |
| ----------- | --------- | ------------------------------------------- | ---------- | ----------------------------- | ------------- | -------- |
| A method    | rust-shop |                                             | cyclomatic | shop::orders::Order::total    | 3             | no       |
| A field     | rust-shop |                                             | cyclomatic | shop::orders::Order::invoices | n/a           | no       |
| An alert    | rust-shop | `add alert cyclomatic function > 2 warning` | cyclomatic | shop::orders::Order::total    | 3             | yes      |

An edge between members, such as `calls`, joins their types at `type` level, and keeps the
members it came from. `from members?` and `to members?` read them. Like any chain, a joined
edge is only as sure as its weakest relation.

[//]: # "decisionTable Core"

| # situation | workspace | level | relations | from                | to                     | from members? | to members?    |
| ----------- | --------- | ----- | --------- | ------------------- | ---------------------- | ------------- | -------------- |
| Calls       | rust-shop | type  | calls     | shop::orders::Order | shop::billing::Invoice | total         | is_free, total |
| Whole types | rust-shop | type  | composes  | shop::orders::Order | shop::billing::Invoice | none          | none           |

[//]: # "decisionTable Core"

| # situation   | workspace | level | relations | from                | to                     | edge confidence? |
| ------------- | --------- | ----- | --------- | ------------------- | ---------------------- | ---------------- |
| A joined edge | rust-shop | type  | calls     | shop::orders::Order | shop::billing::Invoice | ambiguous        |

## Exporting

A diagram can be exported to read it elsewhere, including a whole project that is too
large for a terminal. See [`cyclops export`](../cli/commands.md#exporting).
