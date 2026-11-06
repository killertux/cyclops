# Terminal Interface

Back to the [specification index](../README.md).

`cyclops` with no command opens the terminal interface. It exists to take a question
about code to an answer you can defend, without leaving the terminal. It holds no
analysis of its own: everything it shows comes from the core, so a future interface
can show the same things.

These pages say how the interface behaves, and each table is a test. What it looks
like is in [the design](design/README.md), which is the reference for layout, color
and glyphs. An implementation needs both.

## The four questions

Every screen serves one of the questions in [IDEA.md](../../IDEA.md), by the same five
steps. A view is where a step happens.

| Question                                    | Orient   | Spot      | Explain | Weigh     | Act             |
| ------------------------------------------- | -------- | --------- | ------- | --------- | --------------- |
| How is this project organized?              | Overview | Map       | Explore | Diagram   | Export the view |
| Where is the risk concentrated?             | Overview | Map       | Why     | Inspector | Mark, copy      |
| What should we refactor first?              | Overview | Find      | Why     | Impact    | Open the editor |
| Is this part better or worse than the rest? | Jump     | Inspector | Alerts  | Diagram   | New alert rule  |

## Concepts

| Term          | Meaning                                                                                        |
| ------------- | ---------------------------------------------------------------------------------------------- |
| **View**      | One of Overview, Map, Explore, Diagram, Find, Alerts and Status. One is showing at a time.     |
| **Focus**     | The symbol a view is about. It stays the same when the view changes.                           |
| **Selection** | The row, tile or node under the cursor. Keys that act on a symbol act on it.                   |
| **Lens**      | The focus, the metric, the filter and the test code setting. All four carry across every view. |
| **Inspector** | A pane about the selection, with the sections Metrics, Why, Relations, Clones and Impact.      |
| **Trail**     | The symbols you jumped to, in order, like Vim's jump list.                                     |
| **Mark**      | A symbol saved under a letter, for this session.                                               |
| **Mode**      | What keys mean now: NORMAL, COMMAND, FILTER, SEARCH, EDIT, CONFIRM or ANALYZING.               |

## Rules

- No screen shows a combined score, in any form.
- A value that does not apply shows `n/a`, never 0.
- Warm colors follow [heat](../metrics/catalog.md#heat), so a neutral metric is never warm.
- A guess looks like a guess, and old data is marked as old.
- Anything the mouse can do has a key. Everything clickable shows its key.
- The interface never blocks. You can keep browsing while analysis runs.

## Pages

- [The shell](shell.md): starting, views, the status line, sizes, and quitting.
- [Keys](keys.md): the key map, the Space menu, and counts.
- [Mouse](mouse.md): clicks, menus, and what they map to.
- [The lens](lens.md): metric, filter, and test code across views.
- [Navigation](navigation.md): focus, the trail, marks, and jumping.
- [Analysis](analysis.md): first run, browsing during analysis, and refreshing.
- [Overview](overview.md): where to look first.
- [Map](map.md): size and risk at a glance.
- [Explore](explore.md): the tree and the inspector.
- [Why](why.md): the reason behind a number.
- [Diagram](diagram.md): neighbors and impact.
- [Find](find.md): filtering and ranking.
- [Alerts](alerts.md): rules, hits, and editing them.
- [Status](status.md): how far to trust what you see.
- [Themes](themes.md): dark, light, and no color.
- [The design](design/README.md): layout, tokens, glyphs and screens.

These pages use the `Tui` fixture, described in
[how these pages work](../writing-specs.md#tui-inputs).
