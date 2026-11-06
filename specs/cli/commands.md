# Commands

Back to the [specification index](../README.md).

The command line gives scripts and CI everything the core does. Data goes to stdout
and errors go to stderr, so that output can be piped. The `stdout has?` and
`stderr has?` columns are text that must appear, and `nothing` means it stays empty.

`cyclops` with no command opens the [terminal interface](../tui/README.md).

| Exit code | Meaning                               |
| --------- | ------------------------------------- |
| 0         | It worked.                            |
| 1         | `cyclops check` found failing alerts. |
| 2         | Cyclops could not do what was asked.  |

## Choosing the project

Commands work on the current folder, or the one given with `-C`.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation        | workspace | command                      | exit code? | stdout has? | stderr has?                |
| ------------------ | --------- | ---------------------------- | ---------- | ----------- | -------------------------- |
| The current one    | rust-shop | `cyclops analyze`            | 0          | 6 files     | nothing                    |
| Another one        | rust-shop | `cyclops -C src analyze`     | 0          | 5 files     | nothing                    |
| Does not exist     | rust-shop | `cyclops -C nowhere analyze` | 2          | nothing     | Project not found: nowhere |
| An unknown command | rust-shop | `cyclops nonsense`           | 2          | nothing     | nonsense                   |

## Analyzing

`cyclops analyze` refreshes the cache and prints a summary. File
[problems](../analysis/parse-failures.md) are reported but do not fail the command, and a
[bad configuration](../project/configuration.md#invalid-configuration) does.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation     | workspace | steps          | command                     | exit code? | stderr has?                             |
| --------------- | --------- | -------------- | --------------------------- | ---------- | --------------------------------------- |
| Everything fine | rust-shop |                | `cyclops analyze`           | 0          | nothing                                 |
| Broken files    | broken    |                | `cyclops analyze`           | 0          | src/bad.rs: syntax error                |
| Nothing to do   | docs-only |                | `cyclops analyze`           | 0          | No source files found.                  |
| Bad settings    | rust-shop | `break config` | `cyclops analyze`           | 2          | .cyclops/config.toml is not valid TOML. |
| Asking for more | rust-shop |                | `cyclops analyze --precise` | 0          | nothing                                 |

## Reporting

`cyclops report` prints every metric. The default format is a table for people, and
JSON is for tools. The JSON is one document with no other text around it.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation | workspace | command                        | exit code? | valid json? | stdout has?                |
| ----------- | --------- | ------------------------------ | ---------- | ----------- | -------------------------- |
| For people  | rust-shop | `cyclops report`               | 0          | no          | shop::orders::Order::total |
| For tools   | rust-shop | `cyclops report --format json` | 0          | yes         | shop::orders::Order::total |

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation    | workspace | command                       | exit code? | stdout has? | stderr has?         |
| -------------- | --------- | ----------------------------- | ---------- | ----------- | ------------------- |
| Unknown format | rust-shop | `cyclops report --format xml` | 2          | nothing     | Unknown format: xml |

## Querying

`cyclops query` runs a [filter](../explore/filters.md), with one symbol on each line.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation  | workspace | command                                                            | exit code? | stdout has?                | stderr has?    |
| ------------ | --------- | ------------------------------------------------------------------ | ---------- | -------------------------- | -------------- |
| A match      | rust-shop | `cyclops query 'kind = method and cyclomatic > 2'`                 | 0          | shop::orders::Order::total | nothing        |
| Sorted       | rust-shop | `cyclops query 'kind = method' --sort 'cyclomatic desc' --limit 1` | 0          | shop::orders::Order::total | nothing        |
| As JSON      | rust-shop | `cyclops query 'kind = interface' --format json`                   | 0          | shop::catalog::Priced      | nothing        |
| No matches   | rust-shop | `cyclops query 'cyclomatic > 99'`                                  | 0          | nothing                    | nothing        |
| A bad filter | rust-shop | `cyclops query 'cyclomatic >'`                                     | 2          | nothing                    | Invalid filter |

## Checking alerts

`cyclops check` evaluates the [alert rules](../explore/alerts.md). It fails when an alert
reaches `check.fail_on`, or the `--fail-on` flag, which is `error` by default.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation      | workspace | steps                                                                  | command                           | exit code? | stdout has?                |
| ---------------- | --------- | ---------------------------------------------------------------------- | --------------------------------- | ---------- | -------------------------- |
| No rules         | rust-shop |                                                                        | `cyclops check`                   | 0          | nothing                    |
| No alerts        | rust-shop | `add alert cyclomatic function > 99 error`                             | `cyclops check`                   | 0          | nothing                    |
| Only warnings    | rust-shop | `add alert cyclomatic function > 2 warning`                            | `cyclops check`                   | 0          | shop::orders::Order::total |
| An error         | rust-shop | `add alert cyclomatic function > 2 error`                              | `cyclops check`                   | 1          | shop::orders::Order::total |
| Stricter by flag | rust-shop | `add alert cyclomatic function > 2 warning`                            | `cyclops check --fail-on warning` | 1          | shop::orders::Order::total |
| Stricter by file | rust-shop | `add alert cyclomatic function > 2 warning; set check.fail_on warning` | `cyclops check`                   | 1          | shop::orders::Order::total |
| The flag wins    | rust-shop | `add alert cyclomatic function > 2 warning; set check.fail_on warning` | `cyclops check --fail-on error`   | 0          | shop::orders::Order::total |

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation  | workspace | steps          | command         | exit code? | stderr has?                             |
| ------------ | --------- | -------------- | --------------- | ---------- | --------------------------------------- |
| Bad settings | rust-shop | `break config` | `cyclops check` | 2          | .cyclops/config.toml is not valid TOML. |

`cyclops check` ignores [your own settings](../project/configuration.md#your-own-settings),
both rules and settings, so that it gives the same answer on every machine, including CI.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation        | workspace | steps                                                                                                                 | command         | exit code? |
| ------------------ | --------- | --------------------------------------------------------------------------------------------------------------------- | --------------- | ---------- |
| A rule of yours    | rust-shop | `add user alert cyclomatic function > 2 error`                                                                        | `cyclops check` | 0          |
| A setting of yours | rust-shop | `add alert cyclomatic function > 2 warning; set user check.fail_on warning`                                           | `cyclops check` | 0          |
| The team's rule    | rust-shop | `add user alert cyclomatic function > 2 warning named Complex; add alert cyclomatic function > 2 error named Complex` | `cyclops check` | 1          |

## Exporting

`cyclops export` prints a [diagram](../explore/diagrams.md) in a format other tools can
read. `--level` is `package`, `module`, `type` or `member`, and `module` is the default.
`--focus` and `--depth` choose where to start, and `--overlay` attaches a metric to each
node.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation | workspace | command                                                                  | exit code? | stdout has?   |
| ----------- | --------- | ------------------------------------------------------------------------ | ---------- | ------------- |
| Mermaid     | rust-shop | `cyclops export --format mermaid`                                        | 0          | graph         |
| DOT         | rust-shop | `cyclops export --format dot`                                            | 0          | digraph       |
| PlantUML    | rust-shop | `cyclops export --format plantuml`                                       | 0          | @startuml     |
| Part of it  | rust-shop | `cyclops export --format mermaid --focus shop::orders --depth 1`         | 0          | shop::billing |
| Packages    | rust-shop | `cyclops export --format mermaid --level package`                        | 0          | shop          |
| Members     | rust-shop | `cyclops export --format dot --level member --focus shop::orders::Order` | 0          | invoices      |
| Scores      | rust-shop | `cyclops export --format mermaid --overlay cyclomatic`                   | 0          | cyclomatic 3  |

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation    | workspace | command                                          | exit code? | stdout has? | stderr has?              |
| -------------- | --------- | ------------------------------------------------ | ---------- | ----------- | ------------------------ |
| Unknown format | rust-shop | `cyclops export --format visio`                  | 2          | nothing     | Unknown format: visio    |
| Unknown level  | rust-shop | `cyclops export --format dot --level nonsense`   | 2          | nothing     | Unknown level: nonsense  |
| Unknown metric | rust-shop | `cyclops export --format dot --overlay nonsense` | 2          | nothing     | Unknown metric: nonsense |

### Class diagrams

At `type` level an export is a real UML class diagram, with the same
[class boxes](../explore/diagrams.md#class-boxes) as the interface: stereotypes, fields,
methods and their visibility. Relation kinds use the notation of each format. A guess is
labelled with its confidence, so that it stays a guess outside Cyclops too.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation       | workspace | command                                                             | exit code? | stdout has?     |
| ----------------- | --------- | ------------------------------------------------------------------- | ---------- | --------------- |
| Mermaid           | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | classDiagram    |
| A member          | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | `+total()`      |
| A stereotype      | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | `<<interface>>` |
| Composition       | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | `*--`           |
| Implementation    | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | `<\|..`         |
| Inheritance       | js-shop   | `cyclops export --format mermaid --level type`                      | 0          | `<\|--`         |
| A guess           | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | ambiguous       |
| Scores            | rust-shop | `cyclops export --format mermaid --level type --overlay cyclomatic` | 0          | cyclomatic 3    |
| PlantUML          | rust-shop | `cyclops export --format plantuml --level type`                     | 0          | interface       |
| A PlantUML member | rust-shop | `cyclops export --format plantuml --level type`                     | 0          | `+total()`      |
| DOT               | rust-shop | `cyclops export --format dot --level type`                          | 0          | total           |

## Test code

`--tests` and `--no-tests` work with every command that analyzes, as
[configuration](../project/configuration.md#precedence) describes.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation | workspace | command                                                   | exit code? | stdout has?                    |
| ----------- | --------- | --------------------------------------------------------- | ---------- | ------------------------------ |
| Left out    | rust-shop | `cyclops query 'kind = function and role = test'`         | 0          | nothing                        |
| Included    | rust-shop | `cyclops query 'kind = function and role = test' --tests` | 0          | shop::notes::tests::says_hello |
