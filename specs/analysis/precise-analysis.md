# Precise Analysis

Back to the [specification index](../README.md).

The syntax layer understands code, but it cannot always know what a name refers to.
Precise analysis uses each language's compiler tooling to replace guessed
[relations](../model/relations.md) with exact ones.

| Language               | Indexer           | Needs                                |
| ---------------------- | ----------------- | ------------------------------------ |
| Rust                   | `rust-analyzer`   | rust-analyzer and a Cargo toolchain  |
| TypeScript, JavaScript | `scip-typescript` | Node, and the project's dependencies |

Precise analysis is slow because it covers the whole project, and indexing a Rust
project may run its build scripts and proc macros. So it only runs when asked.

## When it runs

`semantic.mode` is `off`, `manual`, or `always`. As
[configuration](../project/configuration.md#what-the-file-cannot-do) explains, `always`
is only accepted from your own file, a flag or the environment.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation        | workspace | steps                                     | indexers run? | problems?                                        |
| ------------------ | --------- | ----------------------------------------- | ------------- | ------------------------------------------------ |
| By default         | rust-shop |                                           | no            | none                                             |
| Asked for          | rust-shop | `precise analysis`                        | yes           | none                                             |
| Always             | rust-shop | `env CYCLOPS_SEMANTIC_MODE always`        | yes           | none                                             |
| Turned off         | rust-shop | `set semantic.mode off`                   | no            | none                                             |
| Asked for, but off | rust-shop | `set semantic.mode off; precise analysis` | no            | Precise analysis is turned off for this project. |

## What it changes

An exact answer replaces a guess. See how relations are
[guessed without it](../model/relations.md#calls).

[//]: # "decisionTable Core"

| # situation        | workspace     | steps              | from                       | to                              | confidence? |
| ------------------ | ------------- | ------------------ | -------------------------- | ------------------------------- | ----------- |
| Without it         | rust-shop     |                    | shop::orders::Order::total | shop::billing::Invoice::total   | ambiguous   |
| With it            | rust-shop     | `precise analysis` | shop::orders::Order::total | shop::billing::Invoice::total   | exact       |
| A wrong candidate  | rust-shop     | `precise analysis` | shop::orders::Order::total | shop::orders::Order::total      | none        |
| A single candidate | rust-shop     | `precise analysis` | shop::orders::Order::total | shop::billing::Invoice::is_free | exact       |
| TypeScript         | ts-shop       | `precise analysis` | src/orders::Order::total   | src/billing::Invoice::total     | exact       |
| A Rust re-export   | rust-reexport | `precise analysis` | reexport::app::make        | reexport::inner::Thing          | exact       |

Metrics never need it. Complexity, size and duplication come from the syntax layer,
so they do not change.

[//]: # "decisionTable Core"

| # situation | workspace | steps              | subject                    | cyclomatic? |
| ----------- | --------- | ------------------ | -------------------------- | ----------- |
| Without it  | rust-shop |                    | shop::orders::Order::total | 3           |
| With it     | rust-shop | `precise analysis` | shop::orders::Order::total | 3           |

## Results that are out of date

An index belongs to the code it was made from. Relations from a file that has changed
since then go back to what the syntax layer found, until the next precise analysis.
Cyclops never presents old data as current.

[//]: # "decisionTable Core"

| # situation          | workspace | steps                                                    | from                       | to                            | confidence? | index current? |
| -------------------- | --------- | -------------------------------------------------------- | -------------------------- | ----------------------------- | ----------- | -------------- |
| Still current        | rust-shop | `precise analysis`                                       | shop::orders::Order::total | shop::billing::Invoice::total | exact       | yes            |
| That file changed    | rust-shop | `precise analysis; edit src/orders.rs`                   | shop::orders::Order::total | shop::billing::Invoice::total | ambiguous   | no             |
| Another file changed | rust-shop | `precise analysis; edit src/notes.rs`                    | shop::orders::Order::total | shop::billing::Invoice::total | exact       | no             |
| Indexed again        | rust-shop | `precise analysis; edit src/orders.rs; precise analysis` | shop::orders::Order::total | shop::billing::Invoice::total | exact       | yes            |

## Projects in folders

A repository often holds several projects, and nothing at its root says how to build
them all. Cyclops finds them, and indexes each one on its own.

- For Rust, each folder with a `Cargo.toml`, the outermost first. A folder inside one
  that was indexed is skipped when that index already covers its files, as it does for
  a member of a workspace.
- For TypeScript, each folder with a `tsconfig.json`, except the root of the project
  when there are others, since one there usually only serves editors and would take in
  every package at once. A project with no `tsconfig.json` at all is indexed from its
  root. Node may use up to 16 GB, as a large package needs, unless `NODE_OPTIONS` says
  otherwise.

Since each project is indexed on its own, one that fails leaves the answers of the
others, and its problem names its folder, as in
`Precise analysis failed for Rust in services/shop: ...`. The reason is the line of what
the indexer printed that names the error, such as `Error: no projects`, rather than the
end of a stack trace. The [log](logs.md) has everything it printed.

[//]: # "decisionTable Core"

| # situation            | workspace | steps              | from                              | to                                   | confidence? |
| ---------------------- | --------- | ------------------ | --------------------------------- | ------------------------------------ | ----------- |
| Rust in a folder       | monorepo  | `precise analysis` | shop::orders::Order::total        | shop::billing::Invoice::total        | exact       |
| TypeScript in a folder | monorepo  | `precise analysis` | web/shop/src/orders::Order::total | web/shop/src/billing::Invoice::total | exact       |
| One inside another     | monorepo  | `add web/shop/tools/tsconfig.json; precise analysis` | web/shop/src/orders::Order::total | web/shop/src/billing::Invoice::total | exact       |

A project with Rust code and no `Cargo.toml` has nothing to index it with.

[//]: # "decisionTable Core"

| # situation      | workspace | steps                                 | problems?                                               |
| ---------------- | --------- | ------------------------------------- | ------------------------------------------------------- |
| No Cargo project | rust-shop | `delete Cargo.toml; precise analysis` | Precise analysis skipped for Rust: no Cargo.toml found. |

## When a tool is missing

A missing tool is never fatal. Cyclops says what is missing, keeps the syntax layer's
results, and still uses the indexers it can.

[//]: # "decisionTable Core"

| # situation      | workspace | steps                                         | analyzed? | problems?                                                   |
| ---------------- | --------- | --------------------------------------------- | --------- | ----------------------------------------------------------- |
| No rust-analyzer | rust-shop | `remove tool rust-analyzer; precise analysis` | yes       | Precise analysis skipped for Rust: rust-analyzer not found. |
| No Node          | ts-shop   | `remove tool node; precise analysis`          | yes       | Precise analysis skipped for TypeScript: Node not found.    |
| Not asked for    | rust-shop | `remove tool rust-analyzer`                   | yes       | none                                                        |
