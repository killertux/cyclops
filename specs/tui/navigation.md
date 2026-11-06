# Navigation

Back to the [terminal interface](README.md).

The focus is the symbol a view is about, and the selection is the row, tile or node
under the cursor. The focus stays when the view changes, so any result can be seen from
any other view.

## Moving

In a list or a tree, `j` and `k` move the selection, `gg` and `G` go to the ends, and
`Ctrl-d` and `Ctrl-u` move half a page. The sample's rows are in the order of
[Explore](explore.md#the-tree).

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation      | workspace | size  | keys             | selected?    | focus?       |
| ---------------- | --------- | ----- | ---------------- | ------------ | ------------ |
| Down             | rust-shop |       | `<Space>ej`      | shop::orders | shop::orders |
| Up               | rust-shop |       | `<Space>ejjk`    | shop::orders | shop::orders |
| The top          | rust-shop |       | `<Space>ejjgg`   | shop         | shop         |
| The bottom       | rust-shop |       | `<Space>eG`      | shop::notes  | shop::notes  |
| Stops at the top | rust-shop |       | `<Space>ek`      | shop         | shop         |
| Half a page      | rust-shop | 80x24 | `<Space>e<C-d>`  | shop::notes  | shop::notes  |
| And back         | rust-shop | 80x24 | `<Space>eG<C-u>` | shop         | shop         |

## The same focus everywhere

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation    | workspace | keys                        | view?   | focus?       | has?                |
| -------------- | --------- | --------------------------- | ------- | ------------ | ------------------- |
| To the diagram | rust-shop | `<Space>ej<Space>d`         | Diagram | shop::orders | Neighbors of orders |
| To the map     | rust-shop | `<Space>ej<Space>m`         | Map     | shop::orders | orders              |
| And back       | rust-shop | `<Space>ej<Space>d<Space>e` | Explore | shop::orders | Structure           |

The breadcrumb shows where the focus sits. `-` moves the focus to its parent.

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation | workspace | keys              | focus?              | breadcrumb?         |
| ----------- | --------- | ----------------- | ------------------- | ------------------- |
| The project | rust-shop |                   | shop                | shop                |
| A type      | rust-shop | `<C-p>Order<CR>`  | shop::orders::Order | shop, orders, Order |
| Up a level  | rust-shop | `<C-p>Order<CR>-` | shop::orders        | shop, orders        |
| At the top  | rust-shop | `-`               | shop                | shop                |

## Focus and selection

In a list or a tree the two are the same, so moving the selection moves the focus. A map
and a diagram are drawn around the focus, so there the selection moves on its own, and
the picture stays still until you ask. `c` makes the selection the focus, and so does
`Enter` where it goes down a level, as zooming into a tile or drilling into a module does.
On a class box `Enter` [expands the box](diagram.md#class-boxes), and the focus stays.

Whatever acts on a symbol acts on the selection: `gd`, `gr`, `gc`, `gf`, `A`, `m`, `yy` and
the right-click menu. The inspector shows the selection too.

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation  | workspace | keys                                             | focus?       | selected?     | inspecting?   |
| ------------ | --------- | ------------------------------------------------ | ------------ | ------------- | ------------- |
| In a tree    | rust-shop | `<Space>eKj`                                     | shop::orders | shop::orders  | shop::orders  |
| In a diagram | rust-shop | `<Space>ej<Space>dK<click symbol shop::catalog>` | shop::orders | shop::catalog | shop::catalog |
| In a map     | rust-shop | `<Space>mK/orders<CR>`                           | shop         | shop::orders  | shop::orders  |

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation | workspace | keys                                                        | view?   | focus?                     | section? |
| ----------- | --------- | ----------------------------------------------------------- | ------- | -------------------------- | -------- |
| Impact      | rust-shop | `<Space>ej<Space>d<click symbol shop::catalog>gr`           | Diagram | shop::catalog              | Impact   |
| Why         | rust-shop | `<Space>ej<Space>d<click symbol shop::catalog>gd`           | Explore | shop::catalog::Item::price | Why      |
| A mark      | rust-shop | `<Space>ej<Space>d<click symbol shop::catalog>ma<Space>e'a` | Explore | shop::catalog              | closed   |

## Panes

`Tab` moves to the next pane and `Ctrl-w` with a direction moves to the one that way.
`pane?` is the pane that has the keyboard.

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation  | workspace | keys                    | pane?     |
| ------------ | --------- | ----------------------- | --------- |
| Overview     | rust-shop |                         | metrics   |
| Next         | rust-shop | `<Tab>`                 | ranking   |
| Wraps around | rust-shop | `<Tab><Tab>`            | metrics   |
| A direction  | rust-shop | `<Space>eK<C-w>l`       | inspector |
| And back     | rust-shop | `<Space>eK<C-w>l<C-w>h` | structure |

## The trail

The trail records each jump, like Vim's jump list. `Enter`, `gd`, `gr`, `gc`, a mark and
`Ctrl-p` add to it, and `j` and `k` do not. `Ctrl-o` goes back and `Ctrl-n` goes forward,
and each restores the view as well as the focus. A new jump drops what was ahead.

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation         | workspace | keys                                             | view?    | focus?                     | trail? |
| ------------------- | --------- | ------------------------------------------------ | -------- | -------------------------- | ------ |
| At the start        | rust-shop |                                                  | Overview | shop                       | 1/1    |
| After a jump        | rust-shop | `<C-p>Order::total<CR>`                          | Explore  | shop::orders::Order::total | 2/2    |
| Moving adds nothing | rust-shop | `<Space>ejj`                                     | Explore  | shop::billing              | 1/1    |
| Back                | rust-shop | `<C-p>Order::total<CR><C-o>`                     | Overview | shop                       | 1/2    |
| Forward             | rust-shop | `<C-p>Order::total<CR><C-o><C-n>`                | Explore  | shop::orders::Order::total | 2/2    |
| Back from why       | rust-shop | `<C-p>Order::total<CR>gd<C-o>`                   | Explore  | shop::orders::Order::total | 2/3    |
| A new jump          | rust-shop | `<C-p>Order<CR><C-p>Item<CR><C-o><C-p>notes<CR>` | Explore  | shop::notes                | 3/3    |
| Stops at the end    | rust-shop | `<C-o>`                                          | Overview | shop                       | 1/1    |

## Marks

`m` and a letter saves the selection, and `'` and that letter jumps back to it. Marks last
for the session. An unknown mark says so.

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation  | workspace | keys                    | focus?       | has?            |
| ------------ | --------- | ----------------------- | ------------ | --------------- |
| Jumping back | rust-shop | `<Space>ejmagg'a`       | shop::orders | orders          |
| Across views | rust-shop | `<Space>ejma<Space>o'a` | shop::orders | orders          |
| Two marks    | rust-shop | `<Space>ejmajmbgg'a`    | shop::orders | orders          |
| Unknown      | rust-shop | `'z`                    | shop         | Mark not set: z |

## Jumping and searching

`Ctrl-p` finds any symbol in the project by name, wherever you are, and an exact name
comes first. It stays in the view if that view can show the symbol, and goes to
Explore if it cannot. `/` searches the names the current view shows, starting after
the selection, and `n` and `N` step through the matches.

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation    | workspace | keys                     | view?    | focus?                          |
| -------------- | --------- | ------------------------ | -------- | ------------------------------- |
| From Overview  | rust-shop | `<C-p>is_free<CR>`       | Explore  | shop::billing::Invoice::is_free |
| From a diagram | rust-shop | `<Space>d<C-p>notes<CR>` | Diagram  | shop::notes                     |
| No match       | rust-shop | `<C-p>nothing<CR>`       | Overview | shop                            |
| Cancelling     | rust-shop | `<C-p>notes<Esc>`        | Overview | shop                            |

[//]: # "decisionTable Tui -- snooze until 2099-12-31"

| # situation  | workspace | keys                   | selected?     | has?                       |
| ------------ | --------- | ---------------------- | ------------- | -------------------------- |
| Searching    | rust-shop | `<Space>e/notes<CR>`   | shop::notes   | notes                      |
| The next one | rust-shop | `<Space>e/o<CR>n`      | shop::catalog | catalog                    |
| The last one | rust-shop | `<Space>e/o<CR>nN`     | shop::orders  | orders                     |
| No match     | rust-shop | `<Space>e/nothing<CR>` | shop          | Pattern not found: nothing |
