# How These Pages Work

Back to the [specification index](README.md).

Every page in this folder is both documentation and a temoc test. We always write
the spec first. A new table starts snoozed because the feature does not exist yet,
and then we write the code that makes it pass and remove the snooze.

Run the whole specification from the repository root:

```sh
temoc -c temoc.toml
```

List the tables that are still to be built:

```sh
grep -rn 'snooze until 2099' specs
```

## Reading a test table

Each row is one independent scenario. Columns ending in `?` are what we expect to
see. The other columns are inputs. Columns starting with `#` are comments that
temoc ignores.

An empty input means it is not used, and an empty expected cell means the value is
empty. `none` means an empty list or no value, and
`n/a` means a metric does not apply or cannot be worked out. Ratios have two decimal
places. Lists are separated by commas and in alphabetical order.

## Fixtures

Pages use one of four fixtures.

| Fixture   | What it does for each row                                                                           |
| --------- | --------------------------------------------------------------------------------------------------- |
| `Snippet` | Analyzes one piece of code, as a single file of a fresh project.                                    |
| `Core`    | Copies a sample workspace, takes the steps, analyzes it, and reads the result. It has no interface. |
| `Cli`     | Copies a sample workspace, takes the steps, and runs the real `cyclops` binary.                     |
| `Tui`     | Copies a sample workspace, takes the steps, opens the interface, and presses `keys`.                |

`Core` always analyzes once more after the last step, and its expected columns read
that final analysis.

Every fixture uses a fresh home folder for each row, so that no row can touch or read
the settings of the person running it.

`Tui` runs the interface in memory, with no real terminal, and sends it events. It waits for the analysis to finish before
pressing keys, unless a `pause before` step holds it, and it reads its expected columns
once the interface has settled. It tests behavior only. The look of the interface is
set by [the design](tui/design/README.md).

### Snippet inputs

| Column     | Meaning                                                                                                        |
| ---------- | -------------------------------------------------------------------------------------------------------------- |
| `language` | `rust`, `typescript`, or `javascript`.                                                                         |
| `code`     | The source code. `\n` stands for a line break.                                                                 |
| `symbol`   | The symbol to read. Pages that measure a function read `f` by default, and pages that measure a type read `S`. |

### Core and Cli inputs

| Column      | Meaning                                                  |
| ----------- | -------------------------------------------------------- |
| `workspace` | The folder under [`samples/`](../samples) to start from. |
| `steps`     | What to do first, in order, separated by semicolons.     |
| `subject`   | The symbol or file to read.                              |
| `command`   | The `cyclops` command to run. Only for `Cli`.            |

Pages that need more inputs, such as `filter` or `focus`, describe them.

### Tui inputs

| Column      | Meaning                                                     |
| ----------- | ----------------------------------------------------------- |
| `workspace` | The folder under [`samples/`](../samples) to start from.    |
| `steps`     | What to do before opening the interface.                    |
| `size`      | The terminal size, as `COLUMNSxROWS`. Empty means `120x36`. |
| `keys`      | What the person does, in order.                             |
| `subject`   | The symbol to read, for columns that read one symbol.       |

Each character in `keys` is one key press, so `gd` is `g` and then `d`, and a space is
the Space key. Keys with no character use Vim's notation.

| Written as                                | Meaning                                              |
| ----------------------------------------- | ---------------------------------------------------- |
| `<Space>` `<CR>` `<Esc>` `<Tab>` `<BS>`   | Those keys.                                          |
| `<Up>` `<Down>`                           | The arrow keys.                                      |
| `<C-o>`                                   | Control and a key.                                   |
| `<lt>`                                    | The `<` key.                                         |
| `<click TARGET>`                          | A left click.                                        |
| `<double-click TARGET>`                   | A double click.                                      |
| `<right-click TARGET>`                    | A right click.                                       |
| `<hover TARGET>`                          | The mouse resting over something.                    |
| `<wheel up>` `<wheel down>`               | The mouse wheel, over the main list.                 |
| `<wheel up TARGET>` `<wheel down TARGET>` | The mouse wheel, over a target.                      |
| `<ctrl-wheel up>` `<ctrl-wheel down>`     | The wheel with Control held.                         |
| `<drag divider N>`                        | Dragging the pane divider N columns to the right.    |
| `<wait MS>`                               | Doing nothing for that many milliseconds.            |
| `<resize COLUMNSxROWS>`                   | The terminal changing size.                          |
| `<step STEP>`                             | Taking a [step](#steps) while the interface is open. |
| `<resume>`                                | Letting a paused analysis carry on.                  |

A `TARGET` names what is under the mouse, never a position, so that tests survive
layout changes.

| Target          | Meaning                                                                         |
| --------------- | ------------------------------------------------------------------------------- |
| `tab NAME`      | A tab in the tab bar.                                                           |
| `symbol ID`     | The row, tile or node of a symbol.                                              |
| `breadcrumb ID` | A part of the breadcrumb.                                                       |
| `chip NAME`     | A part of the status line: `metric`, `filter`, `tests`, `alerts` or `problems`. |
| `column NAME`   | A column heading.                                                               |
| `menu LABEL`    | An entry in the open menu or popup, by what it says.                            |
| `more`          | The `+N more` node of a diagram, or the `+N small` tile of a map.               |
| `metric NAME`   | A row of the Overview's metric list.                                            |
| `rule TEXT`     | A rule in the Alerts list, written as `rules?` writes it.                       |
| `problem PATH`  | A row of Status' problems.                                                      |
| `section NAME`  | A section of the inspector.                                                     |
| `line N`        | A line of the source in Why.                                                    |
| `popup`         | Inside the open popup.                                                          |
| `outside`       | Anywhere outside the open popup.                                                |

### Steps

| Written as                         | Meaning                                                   |
| ---------------------------------- | --------------------------------------------------------- |
| `analyze`                          | Run an analysis.                                          |
| `precise analysis`                 | Run an analysis that also asks for the semantic indexers. |
| `add PATH`                         | Create a small valid source file.                         |
| `add binary PATH`                  | Create a file that is not valid UTF-8.                    |
| `edit PATH`                        | Change the contents of a file, keeping its meaning.       |
| `touch PATH`                       | Change the modified time of a file, but not its contents. |
| `delete PATH`                      | Delete a file.                                            |
| `lock PATH`                        | Make a file unreadable.                                   |
| `ignore PATTERN`                   | Add a line to `.gitignore`.                               |
| `set KEY VALUE`                    | Write a setting to `.cyclops/config.toml`.                |
| `set user KEY VALUE`               | Write a setting to `~/.config/cyclops/config.toml`.       |
| `break config`                     | Make `.cyclops/config.toml` invalid TOML.                 |
| `break user config`                | Make `~/.config/cyclops/config.toml` invalid TOML.        |
| `add config comment TEXT`          | Add a comment line to `.cyclops/config.toml`.             |
| `add alert RULE`                   | Add an alert rule. See [Alerts](explore/alerts.md).       |
| `add alert RULE named NAME`        | The same, with a `name`.                                  |
| `add user alert RULE`              | Add an alert rule to `~/.config/cyclops/config.toml`.     |
| `pause before STEP`                | Hold the analysis at a step. Only for `Tui`.              |
| `indexer reports PERCENT`          | Make an indexer report its progress. Only for `Tui`.      |
| `terminal background VALUE`        | Make the terminal report a background. Only for `Tui`.    |
| `env NAME VALUE`                   | Set an environment variable.                              |
| `flag FLAG`                        | Pass a command line flag to the final analysis.           |
| `delete cache`                     | Delete the cache.                                         |
| `corrupt cache`                    | Overwrite the cache with garbage.                         |
| `outdate cache`                    | Give the cache an older schema version.                   |
| `upgrade LANGUAGE adapter`         | Raise the version of a language adapter.                  |
| `remove tool NAME`                 | Make a tool unavailable to Cyclops.                       |
| `git init`                         | Create a repository, with every file in one first commit. |
| `commit PATH N times`              | Add commits that change a file.                           |
| `commit PATH N times M months ago` | The same, with old commit dates.                          |
| `rename FROM TO`                   | Rename a file and commit it.                              |
| `rewrite history`                  | Replace the last commit with a different one.             |
| `cyclops ARGUMENTS`                | Run `cyclops`. Only for `Cli`.                            |

### Expected values

| Column      | Meaning                                                                            |
| ----------- | ---------------------------------------------------------------------------------- |
| `analyzed?` | `yes` if the analysis finished, or `no` if it was refused.                         |
| `problem?`  | Why the analysis or a filter was refused, or `none`.                               |
| `problems?` | What the finished analysis wants you to know, or `none`.                           |
| `files?`    | The files analyzed.                                                                |
| `found?`    | Whether the `subject` or `symbol` is visible.                                      |
| Any metric  | That metric of the `subject` or `symbol`, such as `cyclomatic?` or `instability?`. |

Other columns are described on the page that uses them.

### Looking for text

temoc compares a whole value, so a table that looks for text names it in a `text` column
and asks `includes?`, which is `yes` or `no`. `nothing` as the text asks whether there is
no text at all. A prefix says where to look, as in `stdout text` and `stdout includes?`,
and a table that looks for two texts names the second one `other text`.

| Prefix        | Where it looks                    |
| ------------- | --------------------------------- |
| none          | The screen of the interface.      |
| `stdout`      | What a command printed.           |
| `stderr`      | What a command printed as errors. |
| `config`      | `.cyclops/config.toml`.           |
| `user config` | Your own settings file.           |
| `exported`    | The file an export wrote.         |

### Tui expected values

These read the interface once it has settled. Lists that show an order on screen, such
as `rows?`, are in that order rather than alphabetical.

| Column             | Meaning                                                |
| ------------------ | ------------------------------------------------------ |
| `view?`            | The view showing.                                      |
| `mode?`            | The mode in the status line.                           |
| `focus?`           | The symbol in focus.                                   |
| `selected?`        | The selected row, tile or node.                        |
| `rows?`            | The rows of the main list or tree, in order.           |
| `metric?`          | The metric of the lens.                                |
| `filter?`          | The filter of the lens, or `none`.                     |
| `tests?`           | The test code setting, `on` or `off`.                  |
| `section?`         | The open inspector section, or `closed`.               |
| `panes?`           | The panes showing.                                     |
| `pane?`            | The pane that has the keyboard.                        |
| `popup?`           | The popup that is open, or `none`.                     |
| `popup keys?`      | The keys the open popup lists.                         |
| `trail?`           | The place on the trail, as `POSITION/LENGTH`.          |
| `running?`         | `yes` if the interface is still open.                  |
| `marks?`           | The marks of the `subject`, or `none`.                 |
| `opened?`          | The file and line sent to the editor, or `none`.       |
| `clipboard?`       | What was copied.                                       |
| `rules?`           | The alert rules listed, in order.                      |
| `field?`           | The field of a rule being edited.                      |
| `rule?`            | The rule being edited.                                 |
| `hits?`            | How many symbols that rule would fire on.              |
| `level?`           | The level a diagram shows.                             |
| `depth?`           | The depth of a diagram.                                |
| `relations?`       | The relation kinds a diagram shows, or `all`.          |
| `nodes?`           | The nodes of a diagram.                                |
| `node value?`      | The value on the `subject` node.                       |
| `tiles?`           | The tiles of a map.                                    |
| `largest?`         | The tile with the most lines.                          |
| `tile value?`      | The value on the `subject` tile.                       |
| `inspecting?`      | The symbol the inspector shows.                        |
| `inspector width?` | The width of the inspector, or `closed`.               |
| `breadcrumb?`      | The parts of the breadcrumb, in order.                 |
| `count?`           | How many rows the list has.                            |
| `gutter?`          | The lines of the source with points, and those points. |
| `line?`            | The current line of the source.                        |
| `top?`             | The first line of the source in view.                  |
| `mascot?`          | The pose of the mascot.                                |
| `indexers run?`    | `yes` if the indexers ran.                             |
| `created?`         | A file that now exists, or `none`.                     |
| `theme?`           | The theme in use.                                      |

### Writing a subject

A symbol is written as its module, then its names, separated by `::`.

| Language               | Module                               | Example                         |
| ---------------------- | ------------------------------------ | ------------------------------- |
| Rust                   | The crate and module path.           | `shop::billing::Invoice::total` |
| TypeScript, JavaScript | The file path without its extension. | `src/billing::Invoice::total`   |

A Rust integration test is its own crate, so its module is its path, as in
`tests/orders::starts_empty`. A subject with a file extension, such as `src/orders.rs`,
is a file.

## Sample workspaces

The workspaces live in [`samples/`](../samples), outside this folder so that
running the specification does not run them as tests too.

| Workspace                                       | Used for                                           |
| ----------------------------------------------- | -------------------------------------------------- |
| [`rust-shop`](../samples/rust-shop)             | The main Rust project.                             |
| [`ts-shop`](../samples/ts-shop)                 | The same project in TypeScript.                    |
| [`js-shop`](../samples/js-shop)                 | A similar project in JavaScript.                   |
| [`rust-cycles`](../samples/rust-cycles)         | Dependency cycles in Rust.                         |
| [`ts-cycles`](../samples/ts-cycles)             | Dependency cycles in TypeScript.                   |
| [`rust-split-impl`](../samples/rust-split-impl) | A type with `impl` blocks in several files.        |
| [`rust-reexport`](../samples/rust-reexport)     | Rust re-exports.                                   |
| [`rust-chain`](../samples/rust-chain)           | A guess in the middle of a chain of calls.         |
| [`rust-wide`](../samples/rust-wide)             | A module with many dependents.                     |
| [`rust-uneven`](../samples/rust-uneven)         | One large file, small ones, and a getter.          |
| [`rust-crates`](../samples/rust-crates)         | A workspace of one large crate and six small ones. |
| [`ts-resolution`](../samples/ts-resolution)     | TypeScript path aliases and barrel files.          |
| [`clones`](../samples/clones)                   | Duplicated code.                                   |
| [`broken`](../samples/broken)                   | Files with syntax errors.                          |
| [`mixed`](../samples/mixed)                     | One file of every kind Cyclops recognizes.         |
| [`docs-only`](../samples/docs-only)             | A project with no source code.                     |

The three shop projects share one dependency graph, which many pages rely on:

```
orders -> billing -> catalog
orders -> catalog
notes (no dependencies)
```

## Writing a new page

- Describe the behavior in plain language first, then show it with a table.
- Keep each row independent, so that it can run alone.
- Say what Cyclops does, never how it does it.
- Mark every new table `-- snooze until 2099-12-31`.
- Add a link to the page from the [index](README.md).
- Link back to the index at the top of the page.
