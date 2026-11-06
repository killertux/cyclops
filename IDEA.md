# Cyclops

Cyclops helps you understand a codebase and judge its health. Point it at a
project and explore the structure interactively, with quality metrics attached
to the parts of the code they describe.

## The problem

Quality tools usually produce a flat list of numbers or warnings. That tells you
a function is complex, but not where it sits in the system, what depends on it,
or whether anyone is actually changing it. Diagrams have the opposite problem:
they show structure but say nothing about quality.

On a large repository, you need both views at once to answer questions like:

- How is this project organized, and how do its parts depend on each other?
- Where is the risk concentrated?
- What should we refactor first, and what would that affect?
- Is this part of the code in better or worse shape than the rest?

## What Cyclops does

Cyclops analyzes a project and builds a structural model of it, like a UML
diagram, with measurements attached. You can then:

- **Navigate** from the whole project down through packages, modules and types
  to individual functions and their source.
- **See metrics on the diagram**, for example coloring every module by
  instability or every type by cohesion.
- **Filter** by any metric, such as "functions with cyclomatic complexity above
  15" or "modules that are part of a dependency cycle".
- **Set alerts** on the thresholds that matter for this project, and see them
  both while browsing and in automated checks.
- **Include or exclude test code**, so test files and fixtures don't distort the
  picture unless you want them to.
- **Ask for precise analysis** when you need it, replacing guessed relationships
  with ones proven by real compiler tooling.

## Metrics

Metrics are always reported separately. Cyclops deliberately has no combined
health score, because a single number hides the cause and is easy to game.

| Area | What it tells you |
|---|---|
| Cyclomatic complexity | How many independent paths run through a function |
| Cognitive complexity | How hard a function is for a person to follow |
| Size | Lines of code, function length, parameter count, nesting depth |
| Duplication (DRY) | Repeated code, and how much of each file or module it covers |
| Cohesion | Whether a type's members belong together (LCOM) |
| Coupling | Who depends on whom, instability, abstractness, distance from the main sequence |
| Dependency cycles | Groups of modules that depend on each other |
| Hotspots | Code that is both complex and frequently changed, according to git history |

## Ways to use it

- **Interactive TUI:** the main experience, built for investigating a project.
  Running `cyclops` with no arguments opens it.
- **CLI:** the same analysis without the interface, for scripts and CI. It can
  print reports, run queries, export diagrams, and fail a build when alerts fire.
- **GUI or web UI (future):** the core has no dependency on any interface, so
  these can be added without touching the analysis.

## Languages

Cyclops is language-agnostic by design. Each language is an adapter that
translates source code into a shared model, and every metric works on that
model. Rust, TypeScript and JavaScript come first.

Analysis works in two layers. A fast layer always runs and needs nothing
installed. An optional precise layer uses each language's compiler tooling for
exact relationships, and is slower because it analyzes the whole project.

## Per-project state

Each analyzed project gets a `.cyclops/` folder at its root:

- **Settings** such as excluded paths and alert rules. These are meant to be
  committed, so a team shares the same rules.
- **Cache** of analysis results. This stays local and is never committed.

## Principles

1. **Large repositories are the normal case.** A repeat run only does work for
   what changed.
2. **Understanding comes first.** Metrics exist to help you explore the code,
   not to hand out a grade.
3. **Be honest about accuracy.** Where a relationship is inferred rather than
   proven, Cyclops says so.
4. **Every interface gets the same capabilities.** All logic lives in the core,
   so the TUI, the CLI and any future UI cannot drift apart.

## Not in scope for now

- A combined health score
- Architecture rules, such as "domain must not import infra"
- Plugins or adapters loaded at runtime
- Test coverage import
- Tracking metrics over time
