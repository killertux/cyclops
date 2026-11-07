# Rollups

Back to the [specification index](../README.md).

A metric measured on small symbols also appears on the larger symbols that contain
them, so that a score can be shown at any level. Each metric rolls up in its own way.

| Metric                                   | Rolls up as                  |
| ---------------------------------------- | ---------------------------- |
| `cyclomatic`, `cognitive`, `max_nesting` | The highest of its parts.    |
| `lines`, `comment_lines`, `blank_lines`  | The sum of its parts.        |
| `churn`, `hotspot`                       | The highest of its parts.    |
| `duplication`                            | Measured over all its lines. |
| `lcom`, coupling metrics, `parameters`   | It does not roll up.         |

The exception is a file, where `cyclomatic` is the sum of its functions, as
[hotspots](hotspots.md) use it.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation | workspace | subject                    | cyclomatic? | cognitive? | lines? | lcom? |
| ----------- | --------- | -------------------------- | ----------- | ---------- | ------ | ----- |
| A method    | rust-shop | shop::orders::Order::total | 3           | 3          | 9      | n/a   |
| A type      | rust-shop | shop::orders::Order        | 3           | 3          | 17     | 1     |
| A module    | rust-shop | shop::orders               | 3           | 3          | 19     | n/a   |
| A module    | rust-shop | shop::billing              | 2           | 2          | 18     | n/a   |
| A package   | rust-shop | shop                       | 3           | 3          | 55     | n/a   |
| A type      | ts-shop   | src/orders::Order          | 3           | 3          | 15     | 1     |
| A module    | ts-shop   | src/orders                 | 3           | 3          | 17     | n/a   |
| A package   | ts-shop   | shop                       | 3           | 3          | 43     | n/a   |
