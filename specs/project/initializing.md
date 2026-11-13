# Initializing a Project

Back to the [specification index](../README.md).

`cyclops init` prepares a project by creating a `.cyclops/` folder at its root. It
holds the project's settings, which are meant to be committed, and its cache, which
stays local.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Cli"

| # situation        | workspace | command        | exit code? | created?                                  | languages?                   |
| ------------------ | --------- | -------------- | ---------- | ----------------------------------------- | ---------------------------- |
| A Rust project     | rust-shop | `cyclops init` | 0          | .cyclops/.gitignore, .cyclops/config.toml | rust                         |
| TypeScript project | ts-shop   | `cyclops init` | 0          | .cyclops/.gitignore, .cyclops/config.toml | typescript                   |
| JavaScript project | js-shop   | `cyclops init` | 0          | .cyclops/.gitignore, .cyclops/config.toml | javascript                   |
| Several languages  | mixed     | `cyclops init` | 0          | .cyclops/.gitignore, .cyclops/config.toml | gleam, javascript, rust, typescript |
| No source code     | docs-only | `cyclops init` | 0          | .cyclops/.gitignore, .cyclops/config.toml | none                         |

## Initializing twice

An existing configuration is never overwritten.

[//]: # "decisionTable Cli"

| # situation       | workspace | steps                                  | command        | exit code? | tests included? | stdout text         | stdout includes? |
| ----------------- | --------- | -------------------------------------- | -------------- | ---------- | --------------- | ------------------- | ---------------- |
| Settings are kept | rust-shop | `cyclops init; set tests.include true` | `cyclops init` | 0          | true            | Already initialized | yes              |

## Without initializing

Analysis works without `init`, using the [default settings](configuration.md#defaults).
It creates what it needs for the cache and the [log](../analysis/logs.md), and nothing
else.

[//]: # "decisionTable Cli"

| # situation         | workspace | command           | exit code? | created?                                                                  |
| ------------------- | --------- | ----------------- | ---------- | ------------------------------------------------------------------------- |
| Cache and log only  | rust-shop | `cyclops analyze` | 0          | .cyclops/.gitignore, .cyclops/cache/cyclops.db, .cyclops/logs/cyclops.log |

## What git sees

The generated `.gitignore` keeps the cache and the log out of the repository, so nobody
has to edit their own.

[//]: # "decisionTable Cli"

| # situation       | workspace | steps                    | command           | subject                   | ignored by git? |
| ----------------- | --------- | ------------------------ | ----------------- | ------------------------- | --------------- |
| The cache         | rust-shop | `git init; cyclops init` | `cyclops analyze` | .cyclops/cache/cyclops.db | yes             |
| The log           | rust-shop | `git init; cyclops init` | `cyclops analyze` | .cyclops/logs/cyclops.log | yes             |
| The configuration | rust-shop | `git init; cyclops init` | `cyclops analyze` | .cyclops/config.toml      | no              |

## Inside a Cargo workspace

Cargo takes every folder that a workspace's `members` pattern matches as a crate, hidden
ones too, and stops building when one has no `Cargo.toml`. Cyclops never keeps
`.cyclops/` where that would happen: it stops, and says how to go on. A folder the
workspace leaves out with `exclude` is fine.

[//]: # "decisionTable Cli"

| # situation           | workspace     | command                            | exit code? | created?                                                                                                       | stderr text                                                    | stderr includes? |
| --------------------- | ------------- | ---------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- | ---------------- |
| Taken as a member     | rust-crates   | `cyclops -C crates analyze`        | 2          | none                                                                                                           | the Cargo workspace in ../Cargo.toml would take it as a member | yes              |
| Initializing there    | rust-crates   | `cyclops -C crates init`           | 2          | none                                                                                                           | add "crates/.cyclops" to the workspace's exclude               | yes              |
| Inside a member crate | rust-crates   | `cyclops -C crates/engine analyze` | 0          | crates/engine/.cyclops/.gitignore, crates/engine/.cyclops/cache/cyclops.db, crates/engine/.cyclops/logs/cyclops.log | nothing                                                        | yes              |
| At the workspace root | rust-crates   | `cyclops analyze`                  | 0          | .cyclops/.gitignore, .cyclops/cache/cyclops.db, .cyclops/logs/cyclops.log                                      | nothing                                                        | yes              |
| Left out by it        | rust-excluded | `cyclops -C crates analyze`        | 0          | crates/.cyclops/.gitignore, crates/.cyclops/cache/cyclops.db, crates/.cyclops/logs/cyclops.log                 | nothing                                                        | yes              |
