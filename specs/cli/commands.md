# Commands

Back to the [specification index](../README.md).

The command line gives scripts and CI everything the core does. Data goes to stdout
and errors go to stderr, so that output can be piped. `stdout includes?` says whether
the output has the `stdout text`, and `nothing` as the text means it stays empty. The
`stderr` columns work the same way.

`cyclops` with no command opens the [terminal interface](../tui/README.md).

| Exit code | Meaning                               |
| --------- | ------------------------------------- |
| 0         | It worked.                            |
| 1         | `cyclops check` found failing alerts. |
| 2         | Cyclops could not do what was asked.  |

## Choosing the project

Commands work on the current folder, or the one given with `-C`.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Cli"

| # situation        | workspace | command                      | exit code? | stdout text | stdout includes? | stderr text                | stderr includes? |
| ------------------ | --------- | ---------------------------- | ---------- | ----------- | ---------------- | -------------------------- | ---------------- |
| The current one    | rust-shop | `cyclops analyze`            | 0          | 6 files     | yes              | nothing                    | yes              |
| Another one        | rust-shop | `cyclops -C src analyze`     | 0          | 5 files     | yes              | nothing                    | yes              |
| Does not exist     | rust-shop | `cyclops -C nowhere analyze` | 2          | nothing     | yes              | Project not found: nowhere | yes              |
| An unknown command | rust-shop | `cyclops nonsense`           | 2          | nothing     | yes              | nonsense                   | yes              |

## Analyzing

`cyclops analyze` refreshes the cache and prints a summary. File
[problems](../analysis/parse-failures.md) are reported but do not fail the command, and a
[bad configuration](../project/configuration.md#invalid-configuration) does.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation     | workspace | steps          | command                     | exit code? | stderr text                             | stderr includes? |
| --------------- | --------- | -------------- | --------------------------- | ---------- | --------------------------------------- | ---------------- |
| Everything fine | rust-shop |                | `cyclops analyze`           | 0          | nothing                                 | yes              |
| Broken files    | broken    |                | `cyclops analyze`           | 0          | src/bad.rs: syntax error                | yes              |
| Nothing to do   | docs-only |                | `cyclops analyze`           | 0          | No source files found.                  | yes              |
| Bad settings    | rust-shop | `break config` | `cyclops analyze`           | 2          | .cyclops/config.toml is not valid TOML. | yes              |
| Asking for more | rust-shop |                | `cyclops analyze --precise` | 0          | nothing                                 | yes              |

## Reporting

`cyclops report` prints every metric. The default format is a table for people, and
JSON is for tools. The JSON is one document with no other text around it.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation | workspace | command                        | exit code? | valid json? | stdout text                | stdout includes? |
| ----------- | --------- | ------------------------------ | ---------- | ----------- | -------------------------- | ---------------- |
| For people  | rust-shop | `cyclops report`               | 0          | no          | shop::orders::Order::total | yes              |
| For tools   | rust-shop | `cyclops report --format json` | 0          | yes         | shop::orders::Order::total | yes              |

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation    | workspace | command                       | exit code? | stdout text | stdout includes? | stderr text         | stderr includes? |
| -------------- | --------- | ----------------------------- | ---------- | ----------- | ---------------- | ------------------- | ---------------- |
| Unknown format | rust-shop | `cyclops report --format xml` | 2          | nothing     | yes              | Unknown format: xml | yes              |

## Querying

`cyclops query` runs a [filter](../explore/filters.md), with one symbol on each line.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation  | workspace | command                                                            | exit code? | stdout text                | stdout includes? | stderr text    | stderr includes? |
| ------------ | --------- | ------------------------------------------------------------------ | ---------- | -------------------------- | ---------------- | -------------- | ---------------- |
| A match      | rust-shop | `cyclops query 'kind = method and cyclomatic > 2'`                 | 0          | shop::orders::Order::total | yes              | nothing        | yes              |
| Sorted       | rust-shop | `cyclops query 'kind = method' --sort 'cyclomatic desc' --limit 1` | 0          | shop::orders::Order::total | yes              | nothing        | yes              |
| As JSON      | rust-shop | `cyclops query 'kind = interface' --format json`                   | 0          | shop::catalog::Priced      | yes              | nothing        | yes              |
| No matches   | rust-shop | `cyclops query 'cyclomatic > 99'`                                  | 0          | nothing                    | yes              | nothing        | yes              |
| A bad filter | rust-shop | `cyclops query 'cyclomatic >'`                                     | 2          | nothing                    | yes              | Invalid filter | yes              |

## Checking alerts

`cyclops check` evaluates the [alert rules](../explore/alerts.md). It fails when an alert
reaches `check.fail_on`, or the `--fail-on` flag, which is `error` by default.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation      | workspace | steps                                                                  | command                           | exit code? | stdout text                | stdout includes? |
| ---------------- | --------- | ---------------------------------------------------------------------- | --------------------------------- | ---------- | -------------------------- | ---------------- |
| No rules         | rust-shop |                                                                        | `cyclops check`                   | 0          | nothing                    | yes              |
| No alerts        | rust-shop | `add alert cyclomatic function > 99 error`                             | `cyclops check`                   | 0          | nothing                    | yes              |
| Only warnings    | rust-shop | `add alert cyclomatic function > 2 warning`                            | `cyclops check`                   | 0          | shop::orders::Order::total | yes              |
| An error         | rust-shop | `add alert cyclomatic function > 2 error`                              | `cyclops check`                   | 1          | shop::orders::Order::total | yes              |
| Stricter by flag | rust-shop | `add alert cyclomatic function > 2 warning`                            | `cyclops check --fail-on warning` | 1          | shop::orders::Order::total | yes              |
| Stricter by file | rust-shop | `add alert cyclomatic function > 2 warning; set check.fail_on warning` | `cyclops check`                   | 1          | shop::orders::Order::total | yes              |
| The flag wins    | rust-shop | `add alert cyclomatic function > 2 warning; set check.fail_on warning` | `cyclops check --fail-on error`   | 0          | shop::orders::Order::total | yes              |

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation  | workspace | steps          | command         | exit code? | stderr text                             | stderr includes? |
| ------------ | --------- | -------------- | --------------- | ---------- | --------------------------------------- | ---------------- |
| Bad settings | rust-shop | `break config` | `cyclops check` | 2          | .cyclops/config.toml is not valid TOML. | yes              |

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

| # situation | workspace | command                                                                  | exit code? | stdout text   | stdout includes? |
| ----------- | --------- | ------------------------------------------------------------------------ | ---------- | ------------- | ---------------- |
| Mermaid     | rust-shop | `cyclops export --format mermaid`                                        | 0          | graph         | yes              |
| DOT         | rust-shop | `cyclops export --format dot`                                            | 0          | digraph       | yes              |
| PlantUML    | rust-shop | `cyclops export --format plantuml`                                       | 0          | @startuml     | yes              |
| Part of it  | rust-shop | `cyclops export --format mermaid --focus shop::orders --depth 1`         | 0          | shop::billing | yes              |
| Packages    | rust-shop | `cyclops export --format mermaid --level package`                        | 0          | shop          | yes              |
| Members     | rust-shop | `cyclops export --format dot --level member --focus shop::orders::Order` | 0          | invoices      | yes              |
| Scores      | rust-shop | `cyclops export --format mermaid --overlay cyclomatic`                   | 0          | cyclomatic 3  | yes              |

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation    | workspace | command                                          | exit code? | stdout text | stdout includes? | stderr text              | stderr includes? |
| -------------- | --------- | ------------------------------------------------ | ---------- | ----------- | ---------------- | ------------------------ | ---------------- |
| Unknown format | rust-shop | `cyclops export --format visio`                  | 2          | nothing     | yes              | Unknown format: visio    | yes              |
| Unknown level  | rust-shop | `cyclops export --format dot --level nonsense`   | 2          | nothing     | yes              | Unknown level: nonsense  | yes              |
| Unknown metric | rust-shop | `cyclops export --format dot --overlay nonsense` | 2          | nothing     | yes              | Unknown metric: nonsense | yes              |

### Class diagrams

At `type` level an export is a real UML class diagram, with the same
[class boxes](../explore/diagrams.md#class-boxes) as the interface: stereotypes, fields,
methods and their visibility. Relation kinds use the notation of each format. A guess is
labelled with its confidence, so that it stays a guess outside Cyclops too.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation       | workspace | command                                                             | exit code? | stdout text     | stdout includes? |
| ----------------- | --------- | ------------------------------------------------------------------- | ---------- | --------------- | ---------------- |
| Mermaid           | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | classDiagram    | yes              |
| A member          | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | `+total()`      | yes              |
| A stereotype      | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | `<<interface>>` | yes              |
| Composition       | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | `*--`           | yes              |
| Implementation    | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | `<\|..`         | yes              |
| Inheritance       | js-shop   | `cyclops export --format mermaid --level type`                      | 0          | `<\|--`         | yes              |
| A guess           | rust-shop | `cyclops export --format mermaid --level type`                      | 0          | ambiguous       | yes              |
| Scores            | rust-shop | `cyclops export --format mermaid --level type --overlay cyclomatic` | 0          | cyclomatic 3    | yes              |
| PlantUML          | rust-shop | `cyclops export --format plantuml --level type`                     | 0          | interface       | yes              |
| A PlantUML member | rust-shop | `cyclops export --format plantuml --level type`                     | 0          | `+total()`      | yes              |
| DOT               | rust-shop | `cyclops export --format dot --level type`                          | 0          | total           | yes              |

## Test code

`--tests` and `--no-tests` work with every command that analyzes, as
[configuration](../project/configuration.md#precedence) describes.

[//]: # "decisionTable Cli -- snooze until 2099-12-31"

| # situation | workspace | command                                                   | exit code? | stdout text                    | stdout includes? |
| ----------- | --------- | --------------------------------------------------------- | ---------- | ------------------------------ | ---------------- |
| Left out    | rust-shop | `cyclops query 'kind = function and role = test'`         | 0          | nothing                        | yes              |
| Included    | rust-shop | `cyclops query 'kind = function and role = test' --tests` | 0          | shop::notes::tests::says_hello | yes              |
