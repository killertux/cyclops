# Cyclops Architecture Proposal

This document proposes how to build what [IDEA.md](IDEA.md) describes. Code
samples are illustrative sketches, not final APIs. It covers architecture. The
behavior of Cyclops is defined by the executable pages in [specs/](specs/README.md),
which win if the two ever disagree. The visual design of the TUI is in
[specs/tui/design/](specs/tui/design/README.md).

## 1. Overview

```
             discover        extract            link + enrich       analyze
source files ────────> files ───────> FileFacts ─────────────> Project ───────> Metrics
                                 ▲                      ▲       Model              │
                                 │                      │                          ▼
                              fact cache         semantic index             alerts, queries
                                                   (optional)                      │
                                                          ┌────────────────────────┼─────────┐
                                                          ▼                        ▼         ▼
                                                         TUI                      CLI    GUI / web
                                                                                          (future)
```

Four decisions shape everything else:

1. **Adapters produce facts, and analyzers compute metrics.** An adapter only
   describes what is in the code, using a language-neutral model. No metric
   logic lives in an adapter.
2. **Per-file work is separate from project-wide work.** Extraction is the
   expensive step. It is parallel and depends on one file, so it can be cached
   by content.
3. **Metrics are data, not special cases.** Each metric describes itself, so
   filtering, alerts, diagram overlays and CLI output all work generically.
4. **Accuracy comes in two layers.** A fast syntax layer always runs, and an
   optional semantic layer replaces guessed relationships with exact ones.

## 2. Workspace layout

The boundaries are crates, so the compiler rejects a dependency that points the
wrong way.

```
cyclops/
├── Cargo.toml                    workspace root
└── crates/
    ├── cyclops-domain/           model, metric types, filters, alert rules
    ├── cyclops-application/      use cases, ports, analyzers, read model
    ├── cyclops-lang-rust/        Rust adapter
    ├── cyclops-lang-typescript/  TypeScript and JavaScript adapter
    ├── cyclops-semantic/         runs indexers, reads SCIP into relations
    ├── cyclops-infra/            file walking, git, cache, config, exporters
    ├── cyclops-tui/              ratatui interface
    └── cyclops-cli/              the `cyclops` binary and composition root
fixtures/                         the Slim server that runs the specification
specs/                            the specification, run by temoc
samples/                          the workspaces the specification runs against
```

| Crate | CleanArch layer | May depend on |
|---|---|---|
| `cyclops-domain` | Entities | nothing internal |
| `cyclops-application` | Use cases | domain |
| `cyclops-lang-*`, `cyclops-semantic` | Interface adapters | domain, application |
| `cyclops-infra` | Frameworks and drivers | domain, application |
| `cyclops-tui` | Interface adapters | domain, application |
| `cyclops-cli` | Composition root | everything |

`cyclops-tui` never depends on `cyclops-infra` or on an adapter. It receives a
ready-made application service from the binary. A future web server would follow
the same pattern.

## 3. Language support and accuracy

A better parser alone would not remove the accuracy limits. Parsers such as `syn`
and `oxc_parser` produce a syntax tree, as tree-sitter does. Working out which
`save()` a call refers to needs name resolution and type inference, which only a
compiler-grade tool provides. So the design has two layers.

### Syntax layer: always on

This layer runs in-process, needs nothing installed, and is cached per file.

| Language | Library | Why |
|---|---|---|
| Rust | `ra_ap_syntax` | rust-analyzer's own parser: tolerates broken code and has a typed AST. `syn` is ruled out because one syntax error fails the whole file. |
| TypeScript, JavaScript | `oxc_parser`, `oxc_semantic`, `oxc_resolver` | One toolchain for `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs` and `.cjs`, with real scope and symbol analysis. It also resolves module paths. |

This layer provides all complexity, size and duplication data, plus structure and
best-effort relations.

The two parsers fail differently, and the adapters hide that. `ra_ap_syntax` recovers
from a syntax error and keeps going. `oxc_parser` gives up with an empty program, so the
adapter cuts the file back to the last top level statement it can parse, and keeps what
comes before the error. Code after an error in a TypeScript file is therefore lost, and
its lines still count.

Parsers and tree walks recurse, so very deep code could overflow the stack and stop the
whole run. Analysis workers get a large stack, and a file nested more than 256 brackets
deep is reported as a syntax error without being parsed.

### Semantic layer: optional

This layer runs external indexers that use real compiler analysis, and reads their
output in the **SCIP** format:

| Language | Indexer | Requires |
|---|---|---|
| Rust | `rust-analyzer scip` | rust-analyzer and a Cargo toolchain |
| TypeScript, JavaScript | `scip-typescript`, run with `npx` at a pinned version | Node and the project's dependencies installed |

`scip-typescript` is not installed with Node, so Cyclops runs it with
`npx --yes @sourcegraph/scip-typescript@0.6.1`. The first precise analysis on a machine
downloads it, and later ones use npm's cache.

SCIP records where each symbol is defined and referenced. Relations backed by it become
`Confidence::Exact`. `scip-typescript` also records "implements" as a relationship, but
`rust-analyzer` does not, so for Rust it has to be read from the symbol's name, as in
`catalog/impl#[Item][Priced]price()`. See the [spike results](#indexer-spike-results).

**How an index meets the model.** SCIP knows names and positions, not Cyclops symbols.
A definition becomes the innermost symbol of the same name whose lines hold it, and a
use belongs to the innermost symbol whose lines hold it. Uses of names the project does
not define, such as the standard library, are dropped, and a name with no symbol of its
own, such as a TypeScript constructor, falls back to what it belongs to. What is kept is
only "this symbol uses that one", with the content hash of every file at the time of
indexing. A guessed relation is then proven when something inside its source uses its
target or a part of it, and disproven otherwise. Exact relations are never touched, and
neither are relations from files that changed since the index was made.

SCIP was chosen over the alternatives because it fits the language-agnostic goal:

| Option | Verdict |
|---|---|
| SCIP indexers | **Chosen.** One reader serves both languages and any future language with an indexer. |
| `ra_ap_hir` in-process | Deepest Rust analysis, but the API is unstable and it would only help Rust. Worth revisiting if macro accuracy becomes the main complaint. |
| LSP servers | Answers one query at a time, which is too slow and chatty for a whole-project scan. |
| TypeScript 7 compiler API | Not ready: 7.0 shipped without a stable programmatic API, which is targeted for 7.1. Worth revisiting once it is stable. |

### How the layers combine

- Semantic data only upgrades relations. Without it, Cyclops still works, and
  each relation keeps its syntax-layer confidence.
- Indexing is a slow, whole-project operation, so it runs on request by default
  rather than on every analysis.
- An index belongs to a specific commit. Files changed since then fall back to
  syntax-layer relations until the next index, so results are never silently stale.
- Adapters are still compiled in, as agreed. Only the indexers are external
  processes, and their commands are built in rather than read from config, so a
  repository's committed settings cannot make Cyclops run arbitrary commands.
  `semantic.mode = "always"` is refused in that file for the same reason, and is only
  accepted from your own settings file, a flag or the environment.
- Indexing a Rust project may run its build scripts and proc macros. This is
  another reason it is not automatic, and the feature should warn about it.

### What stays approximate

| Limitation | Syntax layer | With semantic layer |
|---|---|---|
| Method calls on inferred types | Guessed | Exact |
| Trait and interface implementations | Mostly correct | Exact |
| Rust re-exports | Guessed | Exact |
| TS path aliases and barrel files | Handled by `oxc_resolver` | Exact |
| Items generated by Rust macros | Missed | Partly recovered |
| Complexity of logic inside macro calls | Undercounted | Still undercounted |

## 4. Test code

Test code can be included in or excluded from measurement. It is **excluded by
default**, because test duplication and fixtures would otherwise drown out real
findings.

**Classification.** Adapters tag every symbol with a `CodeRole`, because a Rust
file can mix both kinds of code:

| Language | Detected as test code |
|---|---|
| Rust | `#[cfg(test)]` modules, `#[test]` functions, `tests/` and `benches/` folders |
| TypeScript, JavaScript | `*.test.*`, `*.spec.*`, `__tests__/`, and similar |
| Any | Extra paths listed in the project's config |

**Behavior.** Test code is always extracted and cached, and the setting is applied
at analysis time. Switching it never re-parses anything. When excluded, test code
contributes to no metric, including coupling and duplication, and it stays hidden
from navigation and search. The setting can come from config, a CLI flag, or a
toggle during an interactive session. Filters can also target it directly with
`role = test`.

## 5. Domain model

### Facts: what an adapter extracts from one file

```rust
pub struct FileFacts {
    pub path: RelativePath,
    pub language: LanguageId,
    pub lines: LineCounts,            // code, comment, blank
    pub symbols: Vec<Symbol>,
    pub references: Vec<RawReference>, // unresolved, matched by name
    pub fingerprints: Vec<Fingerprint>, // for duplication
}

pub struct Symbol {
    pub id: SymbolId,                 // stable: path plus qualified name
    pub parent: Option<SymbolId>,
    pub kind: SymbolKind,             // Module, Class, Struct, Enum, Interface,
                                      // Function, Method, Field, ...
    pub role: CodeRole,               // Production, Test
    pub name: String,
    pub span: Span,
    pub visibility: Visibility,
    pub body: Option<BodyFacts>,
}

pub struct BodyFacts {
    pub decisions: Vec<DecisionPoint>, // kind, line and nesting depth
    pub parameter_count: u32,
    pub max_nesting: u32,
    pub member_accesses: Vec<String>,  // self.x / this.x, for cohesion
    pub local_calls: Vec<String>,      // self.y() / this.y(), for cohesion
}
```

`DecisionPoint` is the key to keeping complexity language-neutral. Adapters map
`if`, `match` arms, `case`, `&&`, `?` and similar constructs onto a small set of
kinds, with a line and a nesting depth. Cyclomatic and cognitive complexity are then
each written once, against those kinds, and the lines let any interface
[explain a score](specs/explore/explanations.md) next to the source.

### Project model: facts after linking

```rust
pub struct ProjectModel {
    pub symbols: SymbolTable,
    pub graph: RelationGraph,
}

pub struct Relation {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: RelationKind,         // Contains, Imports, Implements, Inherits,
                                    // Composes, Uses, Calls
    pub confidence: Confidence,     // Exact, Inferred, Ambiguous
}
```

Rust traits and TypeScript interfaces both become `Interface` symbols, so one
diagram model covers both languages. `Confidence` lets any interface tell the user
which relationships are proven and which are guessed.

### Metrics

```rust
pub struct MetricDescriptor {
    pub id: MetricId,               // "cyclomatic", "lcom", "instability"
    pub name: &'static str,
    pub description: &'static str,
    pub scopes: &'static [Scope],   // Function, Type, File, Module, Project
    pub direction: Direction,       // LowerIsBetter, Neutral
    pub heat: Option<HeatBands>,    // reference values, none for neutral metrics
    pub rollup: Rollup,             // None, Sum, Max
}

pub struct Measurement {
    pub metric: MetricId,
    pub subject: SymbolId,
    pub value: f64,
}
```

`rollup` is what lets a module show the maximum complexity of its functions, so
a score can appear at any diagram level where it makes sense. Metrics with
`Rollup::None`, such as LCOM, only appear at their own scope. The
[metric catalog](specs/metrics/catalog.md) defines each one. `direction` decides what
"worst" means, and `heat` ties warm colors to fixed reference values, so an interface
never invents its own idea of risk.

## 6. Ports

These traits live in `cyclops-application`. Adapters are compiled in and listed
in a registry that the binary builds at startup.

```rust
pub trait LanguageAdapter: Send + Sync {
    fn id(&self) -> LanguageId;
    fn version(&self) -> u32;       // bump to invalidate this adapter's cache
    fn handles(&self, path: &Path) -> bool;

    /// Per-file, parallel, cacheable.
    fn extract(&self, source: &SourceFile) -> Result<FileFacts, ExtractError>;

    /// Project-wide: turn raw references into relations.
    fn link(&self, facts: &[&FileFacts], ctx: &ProjectContext) -> Vec<Relation>;
}

pub trait SemanticProvider: Send + Sync {
    fn languages(&self) -> &[LanguageId];
    fn availability(&self, ctx: &ProjectContext) -> Availability;
    fn index(&self, ctx: &ProjectContext) -> Result<SemanticIndex, IndexError>;
}

pub trait Analyzer: Send + Sync {
    fn descriptors(&self) -> &[MetricDescriptor];
    fn analyze(&self, input: &AnalysisInput) -> Vec<Measurement>;
}

pub trait FileSource { /* discovers files, respecting ignore rules */ }
pub trait FactStore { /* reads and writes the cache */ }
pub trait HistorySource { /* per-file change history, with each commit's details */ }
pub trait ConfigStore { /* reads both settings files, writes keeping comments */ }
pub trait FileWatcher { /* reports that files changed, so an interface can say stale */ }
pub trait ProgressSink { /* reports pipeline progress to any interface */ }
pub trait DiagramExporter { /* Mermaid, DOT, PlantUML */ }
```

`extract` and `link` are separate because resolution needs the whole project.
Rust `impl` blocks can sit in a different file from their struct, and TypeScript
needs `tsconfig` path aliases and barrel files. Those rules are language-specific
and cannot be decided one file at a time.

`availability` lets Cyclops explain exactly what is missing, such as "Node not
found", instead of failing.

## 7. Analysis pipeline

| Step | Work | Parallel | Cached |
|---|---|---|---|
| 1. Discover | Walk the tree, respecting `.gitignore` and `exclude` | Yes | No |
| 2. Check | Compare size and mtime, then content hash if they differ | Yes | Yes |
| 3. Extract | Parse changed files into `FileFacts` | Yes | Yes |
| 4. Link | Each adapter resolves references into relations | Per language | No, at first |
| 5. Enrich | Upgrade relations from the semantic index, if there is one | Yes | Yes |
| 6. History | Read new commits since the last run | No | Yes |
| 7. Analyze | Run all analyzers, applying the test code setting | Yes | No |
| 8. Evaluate | Apply alert rules to measurements | Yes | No |

Results are published as they become known, so that an interface never waits for the
whole pipeline. Complexity and size only need one file, so they are available straight
after Extract and are marked partial. Metrics that need the whole project, or git
history, report what they are waiting for until their step has finished. The read model
therefore starts empty and fills in, rather than appearing at the end.

Syntax trees are dropped as soon as a file's facts are extracted, so memory scales
with the size of the facts rather than the size of the ASTs. A file that fails to
parse produces a recorded diagnostic and never stops the run.

The core is synchronous, using `rayon`, because the work is CPU-bound. An async
runtime would add complexity without benefit here. Interactive interfaces run
analysis on a worker thread and receive progress through a channel, so they stay
responsive.

## 8. Metric implementation notes

| Metric | Scope | Approach |
|---|---|---|
| Cyclomatic | Function, rolls up | Decision points + 1 |
| Cognitive | Function, rolls up | Decision points weighted by nesting |
| Size | Function to project | Line counts, parameter count, nesting |
| Duplication | File, module | Fingerprint matching, see below |
| Cohesion | Type | LCOM4 from member accesses and local calls |
| Coupling | Module | Afferent and efferent counts, instability, abstractness, distance |
| Cycles | Module | Strongly connected components (Tarjan, via `petgraph`) |
| Hotspots | File, rolls up | Normalized churn × complexity |

**Duplication.** Each adapter emits tokens with identifiers and literals
normalized, so renamed copies still match. Winnowing reduces these to a small set
of fingerprints per file, which are cached with the other facts. Matching is a
global hash lookup, so changing one file only re-fingerprints that file. Clones
are grouped into `CloneGroup` records, so any interface can present the copies
together.

**Cohesion.** LCOM4 counts connected groups of methods, where two methods connect
if they share a field or one calls the other. A result of 1 is cohesive, and
higher values suggest the type could be split. Only instance methods count, and in
Rust only inherent ones, since trait methods would make a cohesive type look split.
It is computed after linking, because a Rust type's methods may span several `impl`
blocks and files.

**Hotspots.** These need git. In a project without git history, the metric is
reported as unavailable and everything else still works.

## 9. The `.cyclops/` folder

```
<project>/.cyclops/
├── config.toml       committed
├── .gitignore        generated by `cyclops init`, contains "cache/"
└── cache/
    └── cyclops.db    never committed
```

The generated `.gitignore` means users don't have to edit their own.

### Configuration

```toml
[project]
languages = ["rust", "typescript", "javascript"]
exclude = ["**/generated/**", "**/*.d.ts", "**/dist/**"]

[tests]
include = false
paths = ["e2e/**"]            # extra paths to treat as test code

[semantic]
mode = "manual"               # off or manual; always is refused in this file

[metrics.duplication]
min_tokens = 50

[metrics.hotspots]
since = "12 months"

[check]
fail_on = "error"             # the severity that makes `cyclops check` fail

[[alerts]]
name = "Very complex function"
metric = "cyclomatic"
scope = "function"
when = "> 15"
severity = "warning"

[[alerts]]
name = "Low cohesion in core"
metric = "lcom"
scope = "type"
when = "> 3"
severity = "error"
paths = ["src/core/**"]
```

Each user can also keep settings in `~/.config/cyclops/config.toml`, or the same path
under `XDG_CONFIG_HOME`. It has the same format, holds personal settings such as the
theme, and is merged with the project's file one setting at a time. It is not in
`~/.cyclops` so that it can never collide with the `.cyclops/` of a project in the home
folder.

Precedence follows the usual order: CLI flags, then environment variables, then
`.cyclops/config.toml`, then your own settings, then built-in defaults.

`cyclops check` ignores your own settings completely, so that a build gives the same
answer on every machine.

### Cache

| Data | Key | Invalidated by |
|---|---|---|
| File facts | Content hash, adapter version, schema version | File change, adapter upgrade |
| Semantic index | Commit, indexer version | New index requested |
| Git history | Last processed commit | Rewritten history |
| Metadata | Path | Size or mtime change |

Measurements and alerts are not part of the facts key, so editing a threshold or
switching test code on or off never triggers a re-parse.

**Storage: SQLite via `rusqlite`, with facts stored as `postcard`-encoded blobs.**
It is a single file, handles concurrent readers, is proven at this scale, and can
be inspected with standard tools. It sits behind `FactStore`, so it can be
replaced if benchmarks disagree. The cache is disposable: if it is corrupted or
its schema is outdated, Cyclops deletes it and rebuilds.

## 10. Read model and queries

All interfaces use one service, which is what keeps them equal in capability:

```rust
pub trait ProjectQueries {
    fn children(&self, of: Option<&SymbolId>) -> Vec<SymbolSummary>;
    fn details(&self, id: &SymbolId) -> Option<SymbolDetails>;
    fn search(&self, filter: &Filter, sort: &Sort) -> Page<SymbolSummary>;
    fn diagram(&self, request: &DiagramRequest) -> DiagramView;
    fn alerts(&self, filter: &AlertFilter) -> Vec<AlertHit>;
    fn clones(&self, id: &SymbolId) -> Vec<CloneGroup>;
    fn impact(&self, id: &SymbolId) -> Impact;
    fn rank(&self, id: &SymbolId, metric: &MetricId) -> Option<Rank>;
    fn spread(&self, level: Level, metric: &MetricId) -> Spread;
    fn worst(&self, metric: &MetricId) -> Worst;
    fn explain(&self, id: &SymbolId, metric: &MetricId) -> Explanation;
}
```

**Filters** are a small expression language, parsed in the domain and shared by
every interface:

```
kind = function and cyclomatic > 15
kind = module and in_cycle = true
path ~ "src/core/**" and lcom > 3
role = test and duplication > 20
```

**`DiagramView`** is a plain description of nodes, edges and overlay values. At type
level a node also carries its stereotype and its members with their visibility, and an
edge carries its kind, its confidence and the members it joins. It has
no coordinates. `DiagramRequest` carries the focus symbol, depth, relation kinds,
the metric overlay and an optional filter. Each interface handles its own layout.

## 11. Interactive feature set

This section defines what the interactive interface must do, and deliberately says
nothing about layout, screens or key bindings. Those are specified in
[specs/tui/](specs/tui/README.md).

**Understand the project**
- See a project summary: size, languages, the worst offenders per metric, and
  active alerts.
- Browse the hierarchy from package to module to type to function.
- See any symbol's metrics, relations, source, and duplicates.
- Open a symbol's location in the user's editor.

**Explore the diagram**
- View structure at package, module and type level, with members in class boxes, and
  move between them.
- Focus on one symbol and its neighbors to a chosen depth, since a whole large
  repository cannot be read at once.
- Choose which relation kinds are shown.
- Show any applicable metric on the diagram, and change it during the session.
- Tell apart alerted symbols, dependency cycles, and proven or inferred relations.
- Export the current view or the whole project.

**Investigate with metrics**
- Rank symbols by any metric.
- Filter any list or diagram with a filter expression, and clear it again.
- Move from a result to the same symbol in any other view, keeping that context.
- See why a value is high, such as the decision points behind a complexity score,
  or the commits behind a hotspot.

**Alerts**
- See all alert hits, grouped by rule or by location.
- Create, edit and disable alert rules from the interface, saved to
  `.cyclops/config.toml`.

**Control the analysis**
- See progress and keep browsing while analysis runs.
- Refresh after code changes, reprocessing only what changed.
- Switch test code on or off.
- Start precise analysis, see which tools are missing, and see how current the
  semantic index is.
- See files that failed to parse, and metrics that are unavailable and why.

**How it is built**

The interface uses `ratatui` and `crossterm`, as a model, update and view loop. It holds
no analysis: a view asks `ProjectQueries` for what it shows, and a key turns into an
update of its state. Only three things are specific to a terminal: laying out a treemap
on a grid of cells, laying out a diagram on one, and mapping a mouse position back to a
symbol. Analysis runs on a worker thread and streams progress and results through a
channel, so that the interface never blocks.

The update never does work itself. It returns effects, such as "analyze" or "save the
settings", and whoever runs the interface performs them and reports back: the terminal
runner on worker threads, and the `Tui` fixture straight away, with a clock that only
moves when a row waits. Each frame records where it drew everything the mouse can point
at, so a click is mapped back to a symbol, a tab or a menu entry, and the fixture can
click a target by name. Questions that are analysis, such as the order of the tree,
finding a symbol by name or the worst function inside a module, are queries of the core.
`crossterm` is used through `ratatui`'s re-export, so the two never disagree on a
version, and `notify` lives in `cyclops-infra`, behind the `Backend` the binary gives
the interface.

## 12. CLI

Built with `clap`. Data goes to stdout and errors go to stderr.

```
cyclops                              open the TUI
cyclops init                         create .cyclops/
cyclops analyze                      refresh the cache and print a summary
cyclops analyze --precise            also run the semantic indexers
cyclops report --format json         full report (table, json)
cyclops query 'cyclomatic > 15'      run a filter
cyclops check --fail-on warning      evaluate alerts
cyclops export --format mermaid      export a diagram, at any --level
```

`--tests` and `--no-tests` override the test code setting on any analysis command,
and `-C PATH` works on another folder.

| `cyclops check` exit code | Meaning |
|---|---|
| 0 | No alerts at or above the failure severity |
| 1 | Alerts at or above the failure severity fired |
| 2 | Cyclops could not run |

## 13. Key dependencies

| Purpose | Crate |
|---|---|
| Rust parsing | `ra_ap_syntax` |
| TS and JS parsing | `oxc_parser`, `oxc_ast`, `oxc_ast_visit`, `oxc_allocator`, `oxc_span`, `oxc_syntax`, `oxc_semantic`, `oxc_resolver` |
| Semantic index | `scip`, and `protobuf` to decode it, at the version `scip` pins |
| File walking | `ignore` |
| Parallelism | `rayon` |
| Hashing | `blake3` |
| Cache | `rusqlite`, `postcard`, `serde` |
| Git | `gix` |
| Graphs | `petgraph` |
| Config | `toml`, `serde` |
| Interface | `ratatui`, `crossterm`, `clap` |
| Errors | `thiserror` in libraries, `anyhow` in the binary |

`cyclops-domain` should need only `serde` and `thiserror`.

The terminal interface also needs these, which are approved:

| Purpose | Candidate | Why the list above is not enough |
|---|---|---|
| Saying `stale` when files change | `notify` | The standard library cannot watch files |
| Saving a rule and keeping comments | `toml_edit` | `toml` drops comments when it writes |
| Copying with `yy` | `arboard` | Terminal escape codes for this are blocked by some terminals |
| Finding the terminal's background | `terminal-colorsaurus` | `crossterm` has no query for it |

The `ra_ap_*` and `oxc_*` crates release often and break their APIs, so each family
should be pinned to an exact version and upgraded deliberately. They stay inside
their adapter crates, so an upgrade cannot reach the rest of the system.

## 14. Known limitations and risks

| Risk | Impact | Mitigation |
|---|---|---|
| Indexers don't scale or are unmaintained | Precise analysis is too slow or breaks | [Spike done](#indexer-spike-results); the syntax layer stands alone; revisit `ra_ap_hir` and the TS 7 API |
| `scip-typescript` needs the project's dependencies installed | Precise analysis is unavailable on a fresh clone | Report it through `availability`; fall back to the syntax layer |
| Rust macros are opaque to the syntax layer | Logic inside macro calls is undercounted | Document it; flag affected functions in diagnostics |
| Parser crate API churn | Upgrades take work | Exact pins, contained in adapter crates |
| Readable graphs in a terminal | The core feature could disappoint | Dedicated design work, tested on a real repository |
| Duplication noise | Generated code swamps real findings | Default excludes, `min_tokens`, per-path config |
| Link step on huge repos | Warm runs may still be slow | Measure first, then cache per-package if needed |

## 15. Roadmap

Everything in the specification is part of the first version, including the whole
terminal interface. It is built in batches: each batch makes its tables pass and removes
their snoozes, so `grep -rn 'snooze until 2099' specs` always shows what is left.

| Batch | Delivers | Spec pages | Proves |
|---|---|---|---|
| 1. Foundation | Workspace, domain model, pipeline, Rust extraction, basic CLI, indexer spike | `model/symbols`, `metrics/catalog`, `cyclomatic-complexity`, `cognitive-complexity`, `size`, `analysis/finding-files`, `analysis/parse-failures` | The model works end to end, and SCIP is viable |
| 2. TypeScript and JavaScript | oxc adapter, test code | The pages above for both languages, `analysis/test-code` | The abstraction really is language-neutral |
| 3. Settings and cache | `.cyclops/`, both settings files, fact cache | `project/*`, `analysis/caching` | Repeat runs only pay for what changed |
| 4. Graph | Linking, relations, graph metrics | `model/relations`, `coupling`, `dependency-cycles`, `cohesion`, `rollups` | Syntax-layer relations are useful alone |
| 5. History and duplication | Git history, clones | `hotspots`, `duplication` | The full metric set is in place |
| 6. Exploring | Read model, filters, alerts, ranking, impact, explanations, diagrams, the CLI | `explore/*`, `cli/commands` | Every answer is available without an interface |
| 7. Precise analysis | Semantic layer for both languages | `analysis/precise-analysis` | Relations become exact |
| 8. TUI shell | The frame, keys, lens and navigation | `tui/shell`, `keys`, `lens`, `navigation`, `themes` | The `Tui` fixture can drive the interface |
| 9. TUI reading | The views that list and rank | `tui/overview`, `explore`, `find`, `analysis` | The investigation workflow feels right |
| 10. TUI acting | Reasons, rules and trust | `tui/why`, `alerts`, `status` | A number can be explained and acted on |
| 11. TUI drawing | Map, diagram and mouse | `tui/map`, `diagram`, `mouse` | Scores on structure work in a terminal |
| 12. Finish | The mascot, and matching every design screen | `tui/design` | The interface matches the design |

### Indexer spike results

Run in November 2026, in batch 1. Both indexers are viable, so the semantic layer stands.

| | `rust-analyzer scip` 1.99.1 | `scip-typescript` 0.6.1 |
|---|---|---|
| Real repository | 49 files, 43,336 occurrences | 242 files, 45,092 occurrences |
| Time and peak memory | 10 s, 2.2 GB | 7 s, 1.2 GB |
| Method calls on inferred types | Exact: `invoice.total()` reaches `Invoice::total` | Exact |
| "Implements" | No relationships. Read it from the symbol's name | Recorded as a relationship |
| Progress | Named stages on stderr, no percentage | One line per `tsconfig`, no percentage |
| Maintenance | Weekly releases, ships with rustup | Active, last release this month |
| Needs | The `rust-analyzer` rustup component | Node, and the project's `node_modules` |

- Neither reports a percentage, so `Precise running` shows the time it has been running.
- `rust-analyzer` warns about duplicate symbols, such as one crate that is both a library
  and a binary. The index is still written, and the reader must tolerate duplicates.
- Memory is the main concern. `rust-analyzer` used 1 GB on a 4,000 line workspace and
  2.2 GB on the repository above, so it does not stay flat. Very large repositories were
  not measured, which is left for the benchmarks.

Benchmarks on large repositories come later.
