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
| Several languages  | mixed     | `cyclops init` | 0          | .cyclops/.gitignore, .cyclops/config.toml | javascript, rust, typescript |
| No source code     | docs-only | `cyclops init` | 0          | .cyclops/.gitignore, .cyclops/config.toml | none                         |

## Initializing twice

An existing configuration is never overwritten.

[//]: # "decisionTable Cli"

| # situation       | workspace | steps                                  | command        | exit code? | tests included? | stdout text         | stdout includes? |
| ----------------- | --------- | -------------------------------------- | -------------- | ---------- | --------------- | ------------------- | ---------------- |
| Settings are kept | rust-shop | `cyclops init; set tests.include true` | `cyclops init` | 0          | true            | Already initialized | yes              |

## Without initializing

Analysis works without `init`, using the [default settings](configuration.md#defaults).
It creates what it needs for the cache and nothing else.

[//]: # "decisionTable Cli"

| # situation | workspace | command           | exit code? | created?                                       |
| ----------- | --------- | ----------------- | ---------- | ---------------------------------------------- |
| Cache only  | rust-shop | `cyclops analyze` | 0          | .cyclops/.gitignore, .cyclops/cache/cyclops.db |

## What git sees

The generated `.gitignore` keeps the cache out of the repository, so nobody has to
edit their own.

[//]: # "decisionTable Cli"

| # situation       | workspace | steps                    | command           | subject                   | ignored by git? |
| ----------------- | --------- | ------------------------ | ----------------- | ------------------------- | --------------- |
| The cache         | rust-shop | `git init; cyclops init` | `cyclops analyze` | .cyclops/cache/cyclops.db | yes             |
| The configuration | rust-shop | `git init; cyclops init` | `cyclops analyze` | .cyclops/config.toml      | no              |
