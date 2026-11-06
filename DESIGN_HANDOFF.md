# Design Handoff: TUI Revisions

A review of the TUI design found changes that belong to the design. This is the list.
Please make them in `specs/tui/design/source/`, regenerate with `render.mjs`, and update
`specs/tui/design/README.md`.

## Read this first

The behavior specs have already been changed, so the design needs to catch up with them.
**Do not revert these.** Read them before you start:

| Page | What changed |
| --- | --- |
| [`specs/metrics/catalog.md`](specs/metrics/catalog.md) | New. Every metric, its direction, and its heat. |
| [`specs/tui/navigation.md`](specs/tui/navigation.md#focus-and-selection) | Focus and selection are now separate concepts. |
| [`specs/explore/filters.md`](specs/explore/filters.md#in-a-hierarchy) | One rule for how filters work in trees, maps and diagrams. |
| [`specs/explore/explanations.md`](specs/explore/explanations.md) | Now also explains cohesion, dependencies and cycles. |
| [`specs/tui/why.md`](specs/tui/why.md#what-it-explains) | `gd` follows the current metric. |
| [`specs/tui/alerts.md`](specs/tui/alerts.md) | `cyclops check` ignores personal rules, and `:w user` saves to your own file. |
| [`specs/project/configuration.md`](specs/project/configuration.md) | Your own file moved to `~/.config/cyclops/config.toml`. |

Everything in the TUI ships in the first version, so nothing here is optional. Keep the
principles in the design README: every glyph is one cell, nothing relies on color alone,
and there is no combined score.

## 1. The UML view (most important)

The product brief asks for a UML diagram with scores attached. The design has a module
dependency graph, and that is good, but it stops there. Two things are missing.

**Relation kinds are invisible.** Line style already shows confidence and heavy lines show
cycles, so nothing is left to show whether an edge is `implements`, `inherits`, `composes`,
`uses` or `calls`. With `relations all`, every edge looks the same.

- Encode the kind in the **arrowhead**, since the line style is taken. UML is the guide: a
  hollow triangle for `inherits`, a distinct head for `implements`, a diamond for
  `composes`, and a plain arrow for `uses`, `imports` and `calls`. You choose the final
  glyphs.
- They must work in every confidence style and in `NO_COLOR`.
- Add them to the glyph table and to the diagram legend.

**There is no class view.** At type level a node is only a name and a number, and at
member level members become separate nodes.

- Design a **class box** for type level: the name with its stereotype (`«interface»`,
  `«enum»`), a compartment for fields, a compartment for methods, and the metric of the
  overlay on each member where it applies.
- Say how a large type collapses (for example `+12 more` inside a compartment), and what
  marks a private member.
- Say how `!` and `@` appear on a box and on a member.
- Add a screen: `diagram-types`, with a focus type and its neighbors, using all five kinds.
- Decide whether member level still exists as its own level, or whether drilling into a
  type just expands its box. If this changes behavior, update
  [`specs/tui/diagram.md`](specs/tui/diagram.md) and
  [`specs/explore/diagrams.md`](specs/explore/diagrams.md), with new tables snoozed as
  `-- snooze until 2099-12-31`.
- Core needs to return members and kinds in `DiagramView`, so add tables for what a
  class box must contain.

## 2. Heat

The README computes heat from a value's share of the worst value in the project. That
means every project shows red, even a healthy one, and one outlier makes everything else
look calm. It also breaks "warm means risk".

- Replace the [Heat](specs/tui/design/README.md#heat) section. Core now gives each value a
  [heat level](specs/metrics/catalog.md#heat): `quiet`, `mild`, `high` or `severe`.
- Map those four to `h0` to `h3`, and to the map tiles. Tiles have five tokens, so say what
  `t0` and `t1` mean now.
- Redraw any screen whose colors change. Sample values should look plausible on the new
  scale.

## 3. Neutral metrics

`instability`, `afferent`, `efferent`, `abstractness`, `churn`, `comment_lines` and
`blank_lines` are [neutral](specs/metrics/catalog.md#direction). They have no heat, and they
have no "worst". `lines` only has heat on functions, not on files or modules.

- They must never be warm. Check `overview`, `explore`, `map`, `find` and `diagram`.
- The Overview column `worst symbol` and panel `Worst by ...` are wrong for them. Find
  wording that works for both, or switch to `highest` for neutral metrics.
- `visual-language` should show a neutral value.

## 4. Focus and selection

Now that they differ in Map and Diagram:

- Show the focus and the selection differently on a node and on a tile, including in
  `NO_COLOR`. Today `sel` and the accent border could read as the same thing.
- The inspector shows the selection, so its title should make that clear when the two
  differ.

## 5. Why, for metrics with no source

`gd` now also shows method groups for `lcom`, the modules on each side for the coupling
metrics, a cycle path for `cycle_size`, and the copies for `duplication`. These open in the
Why section and have no design yet.

- Add a screen for each form, or one screen that shows how they share a layout. The `lcom`
  one matters most: each group is a suggestion for splitting a type.

## 6. Fixes to the `why` screen

- `Decision points30` and `Added by nesti+28` are cut off and run into their values.
- The row `1fd77b0 3w  Recompute tax on rate chan` breaks the right border of the pane.
- Remove `Cyclomatic 31 is the 30 points + 1.` It is not true in general: the two metrics
  count differently (a `match` counts once in cognitive, and per arm in cyclomatic).
- Say what the panels show when the metric of the lens is not `cognitive`. Today the title
  is always `Why cognitive is N`.

## 7. Indexing progress

`overview` shows `indexing 38%` and a `Precise 38%` bar. The indexers are external
processes, and they may not report progress.

- Design a state with no percentage: that it is running, and for how long.
- Keep the percentage as the variant for when one is known.
- Update `overview`, `status` and `status-confirm` to match
  [`specs/tui/status.md`](specs/tui/status.md#precise-analysis).

## 8. Text changes

| Screen | Change |
| --- | --- |
| `first-run` | `No .cyclops/ here` becomes `No project settings yet`. The cache creates that folder, so the old text is false. |
| `alerts` | Add the line for rules left out of the exit code: `Leaves out 1 rule of yours.` |
| `alert-rule` | Show `:w user` beside `:w save rule`, and say which file a rule writes to when it is a personal rule. |
| Help and hints | The trail goes forward with `Ctrl-n`. `Ctrl-i` is the same key as `Tab` in most terminals. |

## 9. Small additions

- **`+N more`:** design the popup that lists the rest, which
  [`specs/tui/diagram.md`](specs/tui/diagram.md) calls `more nodes`.
- **Filters on a map or diagram:** a kept tile keeps its whole value, so say whether
  anything shows that a filter is on, beyond the status line.
- **A focus with nothing inside it:** the map shows the parent with that tile selected.
- **Relation picker:** it needs a ticked state for toggling kinds. It has no screen.
- **Overview metrics:** the list is the main metrics only, and the picker has them all.
  Check the picker can show all 18 from the [catalog](specs/metrics/catalog.md).

## When you are done

- `temoc -c temoc.toml` still exits 0.
- Every new table is snoozed, and new pages are linked from an index.
- List anything you changed outside `specs/tui/design/`, so that it can be reviewed.
