# Mouse

Back to the [terminal interface](README.md).

The mouse is a shortcut and a teacher. Each thing it does has a key, and whatever it
touches shows that key, so that using it teaches the keyboard. Nothing is mouse-only.

Capturing the mouse stops a terminal from selecting text, so `Shift` and a drag is left
to the terminal, and `yy` copies a symbol.

## Clicking

A click selects, and a double click does what `Enter` does.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Tui"

| # situation    | workspace | keys                                         | view?   | focus?       | selected?    |
| -------------- | --------- | -------------------------------------------- | ------- | ------------ | ------------ |
| A tab          | rust-shop | `<click tab Diagram>`                        | Diagram | shop         | shop         |
| A row          | rust-shop | `<Space>e<click symbol shop::orders>`        | Explore | shop::orders | shop::orders |
| A double click | rust-shop | `<Space>e<double-click symbol shop::orders>` | Explore | shop::orders | shop::orders |
| A breadcrumb   | rust-shop | `<Space>ej<click breadcrumb shop>`           | Explore | shop         | shop         |

[//]: # "decisionTable Tui"

| # situation          | workspace | keys                                         | rows?                                                                              |
| -------------------- | --------- | -------------------------------------------- | ---------------------------------------------------------------------------------- |
| A double click opens | rust-shop | `<Space>e<double-click symbol shop::orders>` | shop, shop::orders, shop::orders::Order, shop::billing, shop::catalog, shop::notes |

## The status line

Each part of the status line can be clicked.

[//]: # "decisionTable Tui"

| # situation | workspace | keys                  | popup?        | mode?  | tests? |
| ----------- | --------- | --------------------- | ------------- | ------ | ------ |
| The metric  | rust-shop | `<click chip metric>` | metric picker | NORMAL | off    |
| The filter  | rust-shop | `<click chip filter>` | none          | FILTER | off    |
| Test code   | rust-shop | `<click chip tests>`  | none          | NORMAL | on     |

[//]: # "decisionTable Tui"

| # situation  | workspace | keys                    | view?  |
| ------------ | --------- | ----------------------- | ------ |
| The problems | broken    | `<click chip problems>` | Status |
| The alerts   | rust-shop | `<click chip alerts>`   | Alerts |

## The right-click menu

A right click selects what is under it and opens its actions. Each one shows its key.

![The right-click menu](design/screens/context-menu.dark.png)

[//]: # "decisionTable Tui"

| # situation | workspace | keys                                                           | popup?           | popup keys?                       | view?   | section? |
| ----------- | --------- | -------------------------------------------------------------- | ---------------- | --------------------------------- | ------- | -------- |
| Opens       | rust-shop | `<Space>e<right-click symbol shop::orders>`                    | right-click menu | A, Space d, gc, gd, gf, gr, m, yy | Explore | closed   |
| An action   | rust-shop | `<Space>e<right-click symbol shop::orders><click menu Clones>` | none             | none                              | Explore | Clones   |
| With keys   | rust-shop | `<Space>e<right-click symbol shop::orders>j<CR>`               | none             | none                              | Diagram | Impact   |
| Closing     | rust-shop | `<Space>e<right-click symbol shop::orders><Esc>`               | none             | none                              | Explore | closed   |

## Scrolling, dragging and hovering

The wheel scrolls what is under it, and pans a diagram. `Ctrl` and the wheel changes
the diagram's depth. Dragging a divider resizes panes, and dragging a diagram pans it.
Hovering shows what a click would do in the hint line, and changes nothing.

[//]: # "decisionTable Tui"

| # situation | workspace | keys                                  | depth? | selected? | inspector width? | text         | includes? |
| ----------- | --------- | ------------------------------------- | ------ | --------- | ---------------- | ------------ | --------- |
| Depth up    | rust-shop | `<Space>d<ctrl-wheel up>`             | 2      | shop      | closed           | depth 2      | yes       |
| Depth down  | rust-shop | `<Space>d><ctrl-wheel down>`          | 1      | shop      | closed           | depth 1      | yes       |
| A divider   | rust-shop | `<Space>eK<drag divider -5>`          | none   | shop      | 63               | Inspector    | yes       |
| Hovering    | rust-shop | `<Space>e<hover symbol shop::orders>` | none   | shop      | closed           | shop::orders | yes       |
| The wheel   | rust-shop | `<Space>e<wheel down>`                | none   | shop      | closed           | shop         | yes       |
