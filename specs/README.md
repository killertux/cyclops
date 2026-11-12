# Cyclops Specification

Cyclops helps you understand a codebase and judge its health. It builds a structural
model of a project, attaches quality metrics to it, and lets you navigate, filter
and set alerts on the result. See [IDEA.md](../IDEA.md) for the vision and
[PROPOSAL.md](../PROPOSAL.md) for the architecture.

This specification is written in [temoc](https://github.com/killertux/temoc) format.
Every table on these pages is an executable test, so a page describes how Cyclops
behaves and proves it at the same time.

New here? Read [how these pages work](writing-specs.md) and the
[glossary](glossary.md).

## Project

- [Initializing a project](project/initializing.md): the `.cyclops/` folder.
- [Configuration](project/configuration.md): settings, defaults, and precedence.

## Analysis

- [Finding files](analysis/finding-files.md): what gets analyzed, and in which language.
- [Parse failures](analysis/parse-failures.md): what happens with files that are broken.
- [Test code](analysis/test-code.md): recognizing test code, and leaving it out.
- [Caching](analysis/caching.md): only paying for what changed.
- [Precise analysis](analysis/precise-analysis.md): exact relations from compiler tooling.
- [Logs](analysis/logs.md): what each analysis did, to read when something goes wrong.

## The model

- [Symbols](model/symbols.md): how code maps to packages, modules, types and members.
- [Relations](model/relations.md): how symbols connect, and how sure Cyclops is.

## Metrics

- [Catalog](metrics/catalog.md): every metric, its direction, and its heat.
- [Cyclomatic complexity](metrics/cyclomatic-complexity.md): the paths through a function.
- [Cognitive complexity](metrics/cognitive-complexity.md): how hard a function is to follow.
- [Size](metrics/size.md): lines, function length, parameters, and nesting.
- [Duplication](metrics/duplication.md): repeated code.
- [Cohesion](metrics/cohesion.md): whether the members of a type belong together.
- [Coupling](metrics/coupling.md): dependencies, instability, and abstractness.
- [Dependency cycles](metrics/dependency-cycles.md): modules that depend on each other.
- [Hotspots](metrics/hotspots.md): complex code that changes often.
- [Rollups](metrics/rollups.md): how a metric appears at higher levels.

## Exploring

- [Filters](explore/filters.md): finding symbols by any metric.
- [Alerts](explore/alerts.md): thresholds that matter to the project.
- [Diagrams](explore/diagrams.md): structure with metrics attached.
- [Impact](explore/impact.md): what depends on a symbol.
- [Ranking](explore/ranking.md): where a symbol stands among its peers.
- [Explanations](explore/explanations.md): what a value is made of.

## Command line

- [Commands](cli/commands.md): every command, its output, and its exit code.

## Terminal interface

- [The terminal interface](tui/README.md): the concepts, and how the pages fit together.
- [The shell](tui/shell.md): starting, views, the status line, sizes, and quitting.
- [Keys](tui/keys.md): the key map, the Space menu, and counts.
- [Mouse](tui/mouse.md): clicks, menus, and what they map to.
- [The lens](tui/lens.md): metric, filter, and test code across views.
- [Navigation](tui/navigation.md): focus, the trail, marks, and jumping.
- [Analysis](tui/analysis.md): first run, browsing during analysis, and refreshing.
- [Overview](tui/overview.md), [Map](tui/map.md), [Explore](tui/explore.md),
  [Why](tui/why.md), [Diagram](tui/diagram.md), [Find](tui/find.md),
  [Alerts](tui/alerts.md) and [Status](tui/status.md): the views.
- [Themes](tui/themes.md): dark, light, and no color.
- [The design](tui/design/README.md): layout, tokens, glyphs and screens.

## Scope

These are planned for later, and none of them are specified yet:

- **Benchmarks** on large repositories.
