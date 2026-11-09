# Filters

Back to the [specification index](../README.md).

A filter finds symbols by any metric or property. It is a small expression language
that every interface shares. `results?` lists the symbols that match.

## Comparing

Any metric from the [metrics pages](../README.md#metrics) can be compared with `=`,
`!=`, `>`, `>=`, `<` and `<=`.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation     | workspace | filter               | results?                                                            |
| --------------- | --------- | -------------------- | ------------------------------------------------------------------- |
| Greater         | rust-shop | `cognitive > 2`      | shop, shop::orders, shop::orders::Order, shop::orders::Order::total |
| Equal           | rust-shop | `instability = 1`    | shop::orders                                                        |
| Not equal       | rust-shop | `instability != 1`   | shop::billing, shop::catalog                                        |
| Less or equal   | rust-shop | `instability <= 0.5` | shop::billing, shop::catalog                                        |
| Nothing matches | rust-shop | `cyclomatic > 99`    | none                                                                |

A symbol without a value never matches. Above, `shop::notes` has no `instability`, so
it matches neither `=` nor `!=`. Rollups count, which is why modules appear in the
first row. `kind` narrows the search.

## Properties

| Property   | Values                                                            |
| ---------- | ----------------------------------------------------------------- |
| `kind`     | Any [kind](../model/symbols.md). `function` also matches methods. |
| `role`     | `production` or `test`.                                           |
| `language` | `rust`, `typescript`, or `javascript`.                            |
| `path`     | The file of the symbol. `~` matches a glob pattern.               |
| `in_cycle` | `true` or `false`.                                                |

[//]: # "decisionTable Core"

| # situation     | workspace   | steps                    | filter                                   | results?                                                                                                     |
| --------------- | ----------- | ------------------------ | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| A kind          | rust-shop   |                          | `kind = struct`                          | shop::billing::Invoice, shop::catalog::Item, shop::orders::Order                                             |
| Methods only    | rust-shop   |                          | `kind = method and cyclomatic > 1`       | shop::billing::Invoice::total, shop::orders::Order::total                                                    |
| Functions too   | rust-shop   |                          | `kind = function and cyclomatic = 1`     | shop::billing::Invoice::is_free, shop::catalog::Item::price, shop::notes::greeting, shop::orders::Order::add |
| A trait         | rust-shop   |                          | `kind = interface`                       | shop::catalog::Priced                                                                                        |
| A path          | rust-shop   |                          | `kind = struct and path ~ "src/o*"`      | shop::orders::Order                                                                                          |
| A language      | ts-shop     |                          | `kind = class and language = typescript` | src/billing::Invoice, src/catalog::Item, src/orders::Order                                                   |
| Part of a cycle | rust-cycles |                          | `kind = module and in_cycle = true`      | cycles::a, cycles::b, cycles::c, cycles::d, cycles::e                                                        |
| Test code       | rust-shop   | `set tests.include true` | `kind = function and role = test`        | shop::notes::tests::says_hello, tests/orders::starts_empty                                                   |

[//]: # "decisionTable Core"

| # situation | workspace   | steps | filter           | results?    |
| ----------- | ----------- | ----- | ---------------- | ----------- |
| A project   | rust-crates |       | `kind = project` | rust-crates |

## Combining

`and`, `or` and `not` combine conditions, and parentheses group them. `not` binds
tightest, then `and`, then `or`.

[//]: # "decisionTable Core"

| # situation | workspace | filter                                                    | results?                                                                                |
| ----------- | --------- | --------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `and`       | rust-shop | `kind = method and cyclomatic = 2`                        | shop::billing::Invoice::total                                                           |
| `or`        | rust-shop | `kind = struct or kind = interface`                       | shop::billing::Invoice, shop::catalog::Item, shop::catalog::Priced, shop::orders::Order |
| `not`       | rust-shop | `kind = function and not kind = method`                   | shop::notes::greeting                                                                   |
| Precedence  | rust-shop | `kind = interface or kind = struct and path ~ "src/o*"`   | shop::catalog::Priced, shop::orders::Order                                              |
| Parentheses | rust-shop | `(kind = interface or kind = struct) and path ~ "src/o*"` | shop::orders::Order                                                                     |

## Sorting and limiting

Results are in alphabetical order, unless `sort` names a metric followed by `asc` or
`desc`. Ties stay in alphabetical order, and symbols with no value go last. `limit`
keeps the first results.

[//]: # "decisionTable Core"

| # situation | workspace | filter                             | sort              | limit | results?                                                  |
| ----------- | --------- | ---------------------------------- | ----------------- | ----- | --------------------------------------------------------- |
| Descending  | rust-shop | `kind = method and cyclomatic > 1` | `cyclomatic desc` |       | shop::orders::Order::total, shop::billing::Invoice::total |
| Ascending   | rust-shop | `kind = method and cyclomatic > 1` | `cyclomatic asc`  |       | shop::billing::Invoice::total, shop::orders::Order::total |
| The worst   | rust-shop | `kind = method`                    | `cyclomatic desc` | 1     | shop::orders::Order::total                                |

## In a hierarchy

`results?` lists what matches. A view that shows a hierarchy, such as a tree, a map or a
diagram, also needs what leads to a match. So a symbol is **kept** when it matches, or
when something inside it does. `kept?` reads this for the `subject`.

A filter chooses what to look at, and never changes what is measured. A kept module
still shows its whole rollup, and not only the part that matched.

[//]: # "decisionTable Core"

| # situation    | workspace | filter                             | subject                    | kept? |
| -------------- | --------- | ---------------------------------- | -------------------------- | ----- |
| The match      | rust-shop | `kind = method and cyclomatic > 2` | shop::orders::Order::total | yes   |
| Its type       | rust-shop | `kind = method and cyclomatic > 2` | shop::orders::Order        | yes   |
| Its module     | rust-shop | `kind = method and cyclomatic > 2` | shop::orders               | yes   |
| Its package    | rust-shop | `kind = method and cyclomatic > 2` | shop                       | yes   |
| A sibling      | rust-shop | `kind = method and cyclomatic > 2` | shop::orders::Order::add   | no    |
| Another module | rust-shop | `kind = method and cyclomatic > 2` | shop::notes                | no    |

## Invalid filters

A filter that cannot be understood is refused, and says why. It never quietly matches
nothing.

[//]: # "decisionTable Core"

| # situation       | workspace | filter          | problem?                                 |
| ----------------- | --------- | --------------- | ---------------------------------------- |
| An unknown metric | rust-shop | `nonsense > 1`  | Unknown metric or property: nonsense     |
| An unfinished one | rust-shop | `cyclomatic >`  | Invalid filter: expected a value after > |
| An unknown kind   | rust-shop | `kind = widget` | Unknown kind: widget                     |
