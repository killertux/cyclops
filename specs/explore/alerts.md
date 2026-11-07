# Alerts

Back to the [specification index](../README.md).

An alert rule is a threshold that matters to a project. Rules live in
`.cyclops/config.toml`, so that a team shares them.

```toml
[[alerts]]
name = "Very complex function"
metric = "cyclomatic"
scope = "function"
when = "> 2"
severity = "warning"
paths = ["src/core/**"]
enabled = true
```

`name`, `paths` and `enabled` are optional. These tables write a rule on one line, as
`METRIC SCOPE WHEN SEVERITY`, and rules in the `alert` column are separated by
semicolons. A rule can end with `in PATTERN` for its `paths`, or with `disabled`.

`alerts?` lists every alert as its severity and symbol.

## Firing

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation        | workspace       | alert                                                            | alerts?                                                               |
| ------------------ | --------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------- |
| Over the limit     | rust-shop       | `cyclomatic function > 2 warning`                                | warning shop::orders::Order::total                                    |
| At the limit       | rust-shop       | `cyclomatic function >= 2 error`                                 | error shop::billing::Invoice::total, error shop::orders::Order::total |
| Under the limit    | rust-shop       | `cyclomatic function > 99 warning`                               | none                                                                  |
| A module metric    | rust-shop       | `instability module >= 1 warning`                                | warning shop::orders                                                  |
| A type metric      | rust-split-impl | `lcom type > 1 error`                                            | error split::store::Store                                             |
| A rolled up metric | rust-shop       | `cyclomatic module > 2 warning`                                  | warning shop::orders                                                  |
| Several rules      | rust-shop       | `cyclomatic function > 2 warning; instability module >= 1 error` | error shop::orders, warning shop::orders::Order::total                |

Rules follow the same rule as [filters](filters.md): a symbol with no value never
fires, so `shop::notes` has no alert here.

[//]: # "decisionTable Core"

| # situation | workspace | alert                            | alerts?                                      |
| ----------- | --------- | -------------------------------- | -------------------------------------------- |
| No value    | rust-shop | `instability module < 1 warning` | warning shop::billing, warning shop::catalog |

## Narrowing a rule

[//]: # "decisionTable Core"

| # situation        | workspace | steps                    | alert                                             | alerts?                             |
| ------------------ | --------- | ------------------------ | ------------------------------------------------- | ----------------------------------- |
| Inside a path      | rust-shop |                          | `cyclomatic function > 1 error in src/billing.rs` | error shop::billing::Invoice::total |
| Outside a path     | rust-shop |                          | `cyclomatic function > 1 error in src/notes.rs`   | none                                |
| Disabled           | rust-shop |                          | `cyclomatic function > 2 warning disabled`        | none                                |
| Test code left out | rust-shop |                          | `cyclomatic function >= 1 warning in tests/**`    | none                                |
| Test code included | rust-shop | `set tests.include true` | `cyclomatic function >= 1 warning in tests/**`    | warning tests/orders::starts_empty  |

## Reading a symbol

`alerted?` says whether a symbol has an alert, so that any view can mark it.

[//]: # "decisionTable Core"

| # situation | workspace | alert                             | subject                       | alerted? |
| ----------- | --------- | --------------------------------- | ----------------------------- | -------- |
| It fires    | rust-shop | `cyclomatic function > 2 warning` | shop::orders::Order::total    | yes      |
| It does not | rust-shop | `cyclomatic function > 2 warning` | shop::billing::Invoice::total | no       |

## Invalid rules

A rule that can never work is a [configuration problem](../project/configuration.md#invalid-configuration),
and it stops the analysis.

[//]: # "decisionTable Core"

| # situation         | workspace | alert                                  | analyzed? | problem?                                      |
| ------------------- | --------- | -------------------------------------- | --------- | --------------------------------------------- |
| An unknown metric   | rust-shop | `nonsense function > 1 warning`        | no        | Unknown metric: nonsense                      |
| The wrong scope     | rust-shop | `lcom function > 1 warning`            | no        | lcom does not apply to function.              |
| A bad threshold     | rust-shop | `cyclomatic function around 1 warning` | no        | Invalid threshold: around 1                   |
| An unknown severity | rust-shop | `cyclomatic function > 1 loud`         | no        | Unknown severity: loud. Use warning or error. |

Using alerts to fail a build is covered by [`cyclops check`](../cli/commands.md#checking-alerts).
