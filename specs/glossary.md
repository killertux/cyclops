# Glossary

Back to the [specification index](README.md).

**Project.** The folder Cyclops analyzes. Its settings and cache live in `.cyclops/`.
See [Initializing a project](project/initializing.md).

**Package.** A unit of distribution: a Cargo package, or a folder with a
`package.json`.

**Module.** A Rust module, or a TypeScript or JavaScript file or folder. See
[Symbols](model/symbols.md).

**Symbol.** Anything Cyclops can name: a package, module, type, function, method or
field.

**Relation.** A connection between two symbols, such as one type implementing an
interface. See [Relations](model/relations.md).

**Confidence.** How sure Cyclops is of a relation: `exact`, `inferred`, or `ambiguous`.

**Metric.** A named measurement of a symbol. Metrics are always reported separately.
Cyclops has no combined health score. See the [catalog](metrics/catalog.md).

**Direction.** Whether a metric is better when lower, or is neutral. See
[the catalog](metrics/catalog.md#direction).

**Heat.** How worrying a value is: `quiet`, `mild`, `high` or `severe`. See
[the catalog](metrics/catalog.md#heat).

**Rollup.** How a metric of small symbols appears on the larger symbols that contain
them. See [Rollups](metrics/rollups.md).

**Syntax layer.** The analysis that always runs, and needs nothing installed.

**Semantic layer.** The optional analysis that uses external compiler tooling. See
[Precise analysis](analysis/precise-analysis.md).

**Indexer.** An external tool that the semantic layer runs.

**Test code.** Code recognized as tests, which is left out by default. See
[Test code](analysis/test-code.md).

**Clone group.** Pieces of code that are copies of each other. See
[Duplication](metrics/duplication.md).

**Hotspot.** Code that is both complex and frequently changed. See
[Hotspots](metrics/hotspots.md).

**Alert rule.** A threshold on a metric, set by the project. An **alert** is one symbol
breaking one rule. See [Alerts](explore/alerts.md).

**Your own settings.** The file `~/.config/cyclops/config.toml`, merged with the project's
settings. See [Configuration](project/configuration.md#your-own-settings).

**View.** A screen of the terminal interface. See [The shell](tui/shell.md#views).

**Focus.** The symbol a view of the terminal interface is about.

**Selection.** The row, tile or node under the cursor. See
[Navigation](tui/navigation.md#focus-and-selection).

**Lens.** The focus, the metric, the filter and the test code setting, which carry
across every view. See [The lens](tui/lens.md).

**Trail.** The symbols you jumped to, in order. See [Navigation](tui/navigation.md#the-trail).

**Fixture.** Code that connects a test table to the software being tested.

**Snoozed.** A table marked `-- snooze until <date>`. Until that date, its failures
are reported but do not fail the run.
