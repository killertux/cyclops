# Configuration

Back to the [specification index](../README.md).

Settings live in two files, and both are optional because every setting has a default.
`.cyclops/config.toml` belongs to the project and is shared with the team.
`~/.config/cyclops/config.toml` belongs to you and applies to every project.

## Defaults

The `setting` column names the setting to read.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # meaning                     | workspace | setting                        | value?                       |
| ----------------------------- | --------- | ------------------------------ | ---------------------------- |
| Languages to analyze          | rust-shop | project.languages              | gleam, javascript, rust, typescript |
| Paths to leave out            | rust-shop | project.exclude                | none                         |
| Measure test code             | rust-shop | tests.include                  | false                        |
| Extra paths that are tests    | rust-shop | tests.paths                    | none                         |
| When to run precise analysis  | rust-shop | semantic.mode                  | manual                       |
| Shortest clone, in tokens     | rust-shop | metrics.duplication.min_tokens | 50                           |
| How far back to count changes | rust-shop | metrics.hotspots.since         | 12 months                    |
| Severity that fails a check   | rust-shop | check.fail_on                  | error                        |

## Precedence

A command line flag beats an environment variable, which beats the project file,
which beats your own file, which beats the default. Each setting has an environment
variable named after it, such as `CYCLOPS_TESTS_INCLUDE`.

[//]: # "decisionTable Core"

| # situation         | workspace | steps                                                                     | tests included? |
| ------------------- | --------- | ------------------------------------------------------------------------- | --------------- |
| The default         | rust-shop |                                                                           | false           |
| The file            | rust-shop | `set tests.include true`                                                  | true            |
| Environment wins    | rust-shop | `set tests.include true; env CYCLOPS_TESTS_INCLUDE false`                 | false           |
| Flag beats the file | rust-shop | `set tests.include false; flag --tests`                                   | true            |
| Flag beats the rest | rust-shop | `set tests.include true; env CYCLOPS_TESTS_INCLUDE true; flag --no-tests` | false           |

## Your own settings

`~/.config/cyclops/config.toml` has the same format as the project file. The two are merged
one setting at a time, so a project only overrides what it mentions. A list is one
setting, so a project's list replaces yours instead of adding to it. `XDG_CONFIG_HOME`
moves the file, as it does for other tools.

[//]: # "decisionTable Core"

| # situation         | workspace | steps                                                          | tests included? | setting         | value?  |
| ------------------- | --------- | -------------------------------------------------------------- | --------------- | --------------- | ------- |
| Yours alone         | rust-shop | `set user tests.include true`                                  | true            |                 |         |
| The project wins    | rust-shop | `set user tests.include true; set tests.include false`         | false           |                 |         |
| Environment wins    | rust-shop | `set user tests.include true; env CYCLOPS_TESTS_INCLUDE false` | false           |                 |         |
| Other settings stay | rust-shop | `set user tests.include true; set check.fail_on warning`       | true            | check.fail_on   | warning |
| A list is replaced  | rust-shop | `set user project.exclude a/**; set project.exclude b/**`      | false           | project.exclude | b/**    |
| A list of yours     | rust-shop | `set user project.exclude a/**`                                | false           | project.exclude | a/**    |
| Another place       | rust-shop | `env XDG_CONFIG_HOME other; set user tests.include true`       | true            |                 |         |

Alert rules are not one setting. Rules from both files apply together. When a rule in
each file has the same `name`, the project's rule is used. `add user alert` writes a rule
to your own file.

[`cyclops check`](../cli/commands.md#checking-alerts) is the exception. It ignores your own
file completely, so that it gives the same answer on every machine.

[//]: # "decisionTable Core"

| # situation   | workspace | steps                                                                                                                 | alerts?                                                |
| ------------- | --------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Yours alone   | rust-shop | `add user alert cyclomatic function > 2 warning`                                                                      | warning shop::orders::Order::total                     |
| Both apply    | rust-shop | `add user alert cyclomatic function > 2 warning; add alert instability module >= 1 error`                             | error shop::orders, warning shop::orders::Order::total |
| The same name | rust-shop | `add user alert cyclomatic function > 2 warning named Complex; add alert cyclomatic function > 2 error named Complex` | error shop::orders::Order::total                       |

Your own file is not shared, so it is trusted. It is the one file that may set
`semantic.mode` to `always`. Settings under `ui` are personal, so they can only be
set here. See [themes](../tui/themes.md).

[//]: # "decisionTable Core"

| # situation           | workspace | steps                           | analyzed? | problem?                                                   |
| --------------------- | --------- | ------------------------------- | --------- | ---------------------------------------------------------- |
| Always, from your own | rust-shop | `set user semantic.mode always` | yes       | none                                                       |
| Personal, in yours    | rust-shop | `set user ui.theme light`       | yes       | none                                                       |
| Personal, in project  | rust-shop | `set ui.theme light`            | no        | ui.theme can only be set in ~/.config/cyclops/config.toml. |

## Invalid configuration

A problem with the configuration stops the analysis and says what is wrong. Cyclops
never guesses, and never falls back to the defaults. Your own file is held to the same
rules, and the message names it.

[//]: # "decisionTable Core"

| # situation        | workspace | steps                                  | analyzed? | problem?                                                          |
| ------------------ | --------- | -------------------------------------- | --------- | ----------------------------------------------------------------- |
| Not TOML           | rust-shop | `break config`                         | no        | .cyclops/config.toml is not valid TOML.                           |
| Unknown setting    | rust-shop | `set project.unknown 1`                | no        | Unknown setting: project.unknown                                  |
| Wrong type         | rust-shop | `set tests.include maybe`              | no        | tests.include must be true or false.                              |
| Out of range       | rust-shop | `set metrics.duplication.min_tokens 0` | no        | metrics.duplication.min_tokens must be at least 1.                |
| Unknown language   | rust-shop | `set project.languages cobol`          | no        | Unknown language: cobol. Supported: gleam, javascript, rust, typescript. |
| Your own, not TOML | rust-shop | `break user config`                    | no        | ~/.config/cyclops/config.toml is not valid TOML.                  |
| Your own, unknown  | rust-shop | `set user project.unknown 1`           | no        | Unknown setting: project.unknown                                  |

## What the file cannot do

The project file is committed, so it may come from someone you do not trust. It can
never make Cyclops run a command. For the same reason, `always` is refused as a
[precise analysis](../analysis/precise-analysis.md) mode in that file, because indexing
can run a project's build scripts.

[//]: # "decisionTable Core"

| # situation         | workspace | steps                      | analyzed? | problem?                                                                      |
| ------------------- | --------- | -------------------------- | --------- | ----------------------------------------------------------------------------- |
| Always, from a file | rust-shop | `set semantic.mode always` | no        | semantic.mode can only be always in your own file, a flag or the environment. |
