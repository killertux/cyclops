# Metric Catalog

Back to the [specification index](../README.md).

Every metric Cyclops measures, in one place. Filters, alerts, diagrams and every
interface use these names. An interface may show `cycle_size` as `cycle size`.

| Metric          | Measures                       | Measured on    | Page                                      |
| --------------- | ------------------------------ | -------------- | ----------------------------------------- |
| `cyclomatic`    | Paths through code             | Function       | [Cyclomatic](cyclomatic-complexity.md)    |
| `cognitive`     | Difficulty to follow           | Function       | [Cognitive](cognitive-complexity.md)      |
| `max_nesting`   | Deepest control flow           | Function       | [Size](size.md)                           |
| `lines`         | Lines of code                  | Function, file | [Size](size.md)                           |
| `comment_lines` | Lines of comments              | File           | [Size](size.md)                           |
| `blank_lines`   | Blank lines                    | File           | [Size](size.md)                           |
| `parameters`    | Function parameters            | Function       | [Size](size.md)                           |
| `members`       | Fields and methods             | Type           | [Size](size.md)                           |
| `duplication`   | Share of copied lines          | File           | [Duplication](duplication.md)             |
| `lcom`          | Unrelated method groups        | Type           | [Cohesion](cohesion.md)                   |
| `afferent`      | Modules depending on it        | Module         | [Coupling](coupling.md)                   |
| `efferent`      | Modules it depends on          | Module         | [Coupling](coupling.md)                   |
| `instability`   | Share of outgoing dependencies | Module         | [Coupling](coupling.md)                   |
| `abstractness`  | Share of interfaces            | Module         | [Coupling](coupling.md)                   |
| `distance`      | Imbalance of the two           | Module         | [Coupling](coupling.md)                   |
| `cycle_size`    | Modules in its cycle           | Module         | [Dependency cycles](dependency-cycles.md) |
| `churn`         | Commits changing it            | File           | [Hotspots](hotspots.md)                   |
| `hotspot`       | Churn and complexity           | File           | [Hotspots](hotspots.md)                   |

## Direction

A metric's direction says whether a value is good or bad. Most metrics are better when
lower. A **neutral** metric only describes something, and neither end is a problem. A
stable module with many dependents and an unstable entry point are both healthy, which
is why `instability` is neutral and `distance` is the one that signals trouble.

Where these pages say "worst", a neutral metric means its highest value.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core -- snooze until 2099-12-31"

| metric        | direction?      |
| ------------- | --------------- |
| cyclomatic    | lower is better |
| cognitive     | lower is better |
| max_nesting   | lower is better |
| lines         | lower is better |
| comment_lines | neutral         |
| blank_lines   | neutral         |
| parameters    | lower is better |
| members       | lower is better |
| duplication   | lower is better |
| lcom          | lower is better |
| afferent      | neutral         |
| efferent      | neutral         |
| instability   | neutral         |
| abstractness  | neutral         |
| distance      | lower is better |
| cycle_size    | lower is better |
| churn         | neutral         |
| hotspot       | lower is better |

## Heat

Heat says how worrying a value is, so that interfaces can show it the same way. It
comes from fixed reference values rather than from the rest of the project, so a
healthy project stays quiet and a troubled one shows it. Heat is only a guide. The
thresholds a project enforces are its [alert rules](../explore/alerts.md).

| Metric        | `mild` from | `high` from | `severe` from |
| ------------- | ----------- | ----------- | ------------- |
| `cyclomatic`  | 5           | 10          | 20            |
| `cognitive`   | 8           | 15          | 25            |
| `max_nesting` | 3           | 4           | 5             |
| `lines`       | 30          | 60          | 100           |
| `parameters`  | 4           | 6           | 8             |
| `members`     | 15          | 25          | 40            |
| `duplication` | 5           | 15          | 30            |
| `lcom`        | 2           | 3           | 5             |
| `distance`    | 0.30        | 0.50        | 0.70          |
| `cycle_size`  | 2           | 4           | 8             |
| `hotspot`     | 0.20        | 0.40        | 0.70          |

Below `mild`, a value is `quiet`. Neutral metrics and values that are `n/a` have no heat.
`lines` has heat on functions only, because a sum says nothing about a large module.
A rolled up value has the heat of its value.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation      | metric      | scope    | value | heat?  |
| ---------------- | ----------- | -------- | ----- | ------ |
| Quiet            | cyclomatic  | function | 4     | quiet  |
| Mild             | cyclomatic  | function | 5     | mild   |
| High             | cyclomatic  | function | 10    | high   |
| Severe           | cyclomatic  | function | 20    | severe |
| Another metric   | cognitive   | function | 15    | high   |
| A ratio          | distance    | module   | 0.70  | severe |
| A cohesive type  | lcom        | type     | 1     | quiet  |
| A rollup         | cyclomatic  | module   | 12    | high   |
| A neutral metric | instability | module   | 1.00  | none   |
| Another neutral  | afferent    | module   | 60    | none   |
| A long function  | lines       | function | 100   | severe |
| A large module   | lines       | module   | 5000  | none   |
| No value         | lcom        | module   | n/a   | none   |
