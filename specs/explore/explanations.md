# Explanations

Back to the [specification index](../README.md).

A score is only useful if you can see where it came from. An explanation lists what
made up a value, so that any interface can show it next to the code.

## Decision points

`points?` lists what added to a function's
[cognitive complexity](../metrics/cognitive-complexity.md), as the line and the points
it added, and it always adds up to the score. Lines are counted from the start of
the file. `decisions?` is the number of decision points, which is what
[cyclomatic complexity](../metrics/cyclomatic-complexity.md) counts. `nesting?` is how
many of the points come from nesting.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation     | workspace | subject                       | points?      | decisions? | nesting? |
| --------------- | --------- | ----------------------------- | ------------ | ---------- | -------- |
| With nesting    | rust-shop | shop::orders::Order::total    | 15 +1, 16 +2 | 2          | 1        |
| An `if`, `else` | rust-shop | shop::billing::Invoice::total | 11 +1, 13 +1 | 1          | 0        |
| Straight code   | rust-shop | shop::notes::greeting         | none         | 0          | 0        |

`kinds?` groups the decision points of a function by what they are.

[//]: # "decisionTable Snippet"

| # situation  | language   | code                                                       | kinds?             |
| ------------ | ---------- | ---------------------------------------------------------- | ------------------ |
| An `if`      | rust       | `fn f(a: bool) { if a {} }`                                | if 1               |
| A loop in it | rust       | `fn f(a: bool, b: &[u8]) { if a { for _ in b {} } }`       | if 1, loop 1       |
| Match arms   | rust       | `fn f(a: u8) -> u8 { match a { 1 => 1, 2 => 2, _ => 3 } }` | match arm 2        |
| An operator  | rust       | `fn f(a: bool, b: bool) -> bool { a && b }`                | logical operator 1 |
| TypeScript   | typescript | `function f(a: boolean) { if (a) {} }`                     | if 1               |

`points by kind?` adds the cognitive points up by structure.

[//]: # "decisionTable Core"

| # situation     | workspace | subject                       | points by kind? |
| --------------- | --------- | ----------------------------- | --------------- |
| With nesting    | rust-shop | shop::orders::Order::total    | if 2, loop 1    |
| An `if`, `else` | rust-shop | shop::billing::Invoice::total | else 1, if 1    |
| Straight code   | rust-shop | shop::notes::greeting         | none            |

## Nesting

`deepest line?` is where [nesting](../metrics/size.md#nesting) is at its deepest, and
`deepest path?` lists the structures that lead there, from the outside in.

[//]: # "decisionTable Core"

| # situation   | workspace | subject                       | deepest line? | deepest path? |
| ------------- | --------- | ----------------------------- | ------------- | ------------- |
| Two levels    | rust-shop | shop::orders::Order::total    | 16            | loop, if      |
| One level     | rust-shop | shop::billing::Invoice::total | 11            | if            |
| Straight code | rust-shop | shop::notes::greeting         | none          | none          |

## Compared with its file

`in file?` says where a function stands among the functions of its file, for the `metric`.
Rank 1 is the worst, as on the [ranking](ranking.md) page.

[//]: # "decisionTable Core"

| # situation     | workspace | subject                         | metric     | in file? |
| --------------- | --------- | ------------------------------- | ---------- | -------- |
| The longest     | rust-shop | shop::orders::Order::total      | lines      | 1 of 2   |
| The shortest    | rust-shop | shop::billing::Invoice::is_free | lines      | 2 of 2   |
| Most parameters | rust-shop | shop::orders::Order::add        | parameters | 1 of 2   |
| Alone in a file | rust-shop | shop::notes::greeting           | lines      | 1 of 1   |

## Commits

A [hotspot](../metrics/hotspots.md) comes from history. `commits?` lists how many commits
changed a file within `metrics.hotspots.since`, which is the same as its `churn`.
`file complexity?` is the other half of the score.

[//]: # "decisionTable Core"

| # situation | workspace | steps                                                  | subject       | commits? | file complexity? |
| ----------- | --------- | ------------------------------------------------------ | ------------- | -------- | ---------------- |
| Changed     | rust-shop | `git init; commit src/orders.rs 9 times`               | src/orders.rs | 10       | 4                |
| Old commits | rust-shop | `git init; commit src/orders.rs 4 times 13 months ago` | src/orders.rs | 1        | 4                |
| Without git | rust-shop |                                                        | src/orders.rs | n/a      | 4                |

Each commit has its short id, its age and its subject. `latest commit?` reads the most
recent one, and the subject of a commit made by `commit` is `Change PATH`.

[//]: # "decisionTable Core"

| # situation | workspace | steps                                    | subject       | latest commit?       |
| ----------- | --------- | ---------------------------------------- | ------------- | -------------------- |
| The newest  | rust-shop | `git init; commit src/orders.rs 2 times` | src/orders.rs | Change src/orders.rs |

## Cohesion

`groups?` lists the [unrelated groups of methods](../metrics/cohesion.md) of a type,
separated by semicolons. They are what LCOM counts, and each one is a candidate for
splitting the type.

[//]: # "decisionTable Snippet"

| # situation     | language   | code                                                                                                        | groups? |
| --------------- | ---------- | ----------------------------------------------------------------------------------------------------------- | ------- |
| Separate fields | rust       | `struct S { a: u8, b: u8 } impl S { fn x(&self) -> u8 { self.a } fn y(&self) -> u8 { self.b } }`            | x; y    |
| Joined          | rust       | `struct S { a: u8, b: u8 } impl S { fn x(&self) -> u8 { self.a } fn y(&self) -> u8 { self.b + self.x() } }` | x, y    |
| A class         | typescript | `class S { a = 1; b = 2; x() { return this.a; } y() { return this.b; } }`                                   | x; y    |
| No methods      | rust       | `struct S { a: u8 }`                                                                                        | none    |

[//]: # "decisionTable Core"

| # situation  | workspace       | subject             | groups?     |
| ------------ | --------------- | ------------------- | ----------- |
| Across files | rust-split-impl | split::store::Store | grow; label |

`group fields?` lists the fields each group uses, in the same order.

[//]: # "decisionTable Snippet"

| # situation     | language | code                                                                                             | group fields? |
| --------------- | -------- | ------------------------------------------------------------------------------------------------ | ------------- |
| Separate fields | rust     | `struct S { a: u8, b: u8 } impl S { fn x(&self) -> u8 { self.a } fn y(&self) -> u8 { self.b } }` | a; b          |

## Dependencies

`depends on?` and `depended on by?` list the modules behind the
[coupling metrics](../metrics/coupling.md).

[//]: # "decisionTable Core"

| # situation | workspace | subject       | depends on?                  | depended on by?             |
| ----------- | --------- | ------------- | ---------------------------- | --------------------------- |
| The top     | rust-shop | shop::orders  | shop::billing, shop::catalog | none                        |
| The middle  | rust-shop | shop::billing | shop::catalog                | shop::orders                |
| The bottom  | rust-shop | shop::catalog | none                         | shop::billing, shop::orders |
| Alone       | rust-shop | shop::notes   | none                         | none                        |

## Cycles

`cycle path?` shows a way around the [cycle](../metrics/dependency-cycles.md) of a module,
starting from it.

[//]: # "decisionTable Core"

| # situation    | workspace   | subject   | cycle path?                                      |
| -------------- | ----------- | --------- | ------------------------------------------------ |
| A pair         | rust-cycles | cycles::a | cycles::a -> cycles::b -> cycles::a              |
| A ring         | rust-cycles | cycles::d | cycles::d -> cycles::e -> cycles::c -> cycles::d |
| Not in a cycle | rust-cycles | cycles::f | none                                             |

`cycle hops?` gives each step of that path, with its kind and confidence.

[//]: # "decisionTable Core"

| # situation | workspace   | subject   | cycle hops?                                                                |
| ----------- | ----------- | --------- | -------------------------------------------------------------------------- |
| A pair      | rust-cycles | cycles::a | cycles::a -> cycles::b imports exact; cycles::b -> cycles::a imports exact |
| None        | rust-cycles | cycles::f | none                                                                       |
