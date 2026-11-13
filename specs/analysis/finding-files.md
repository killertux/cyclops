# Finding Files

Back to the [specification index](../README.md).

Cyclops analyzes the source files of a project in the languages it knows. Everything
else is skipped without complaint. The `files?` column lists what was analyzed.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation          | workspace | steps                                   | files?                                                                       |
| -------------------- | --------- | --------------------------------------- | ---------------------------------------------------------------------------- |
| Every source file    | js-shop   |                                         | src/billing.mjs, src/catalog.js, src/notes.jsx, src/orders.cjs               |
| Other files          | js-shop   | `add src/notes.txt`                     | src/billing.mjs, src/catalog.js, src/notes.jsx, src/orders.cjs               |
| Hidden folders       | js-shop   | `add .hidden/extra.js`                  | src/billing.mjs, src/catalog.js, src/notes.jsx, src/orders.cjs               |
| Its own folder       | js-shop   | `add .cyclops/cache/extra.js`           | src/billing.mjs, src/catalog.js, src/notes.jsx, src/orders.cjs               |
| Installed packages   | js-shop   | `add node_modules/pkg/index.js`         | src/billing.mjs, src/catalog.js, src/notes.jsx, src/orders.cjs               |
| Build output         | js-shop   | `add target/debug/build.js`             | src/billing.mjs, src/catalog.js, src/notes.jsx, src/orders.cjs               |
| Ignored by git       | js-shop   | `add src/extra.js; ignore src/extra.js` | src/billing.mjs, src/catalog.js, src/notes.jsx, src/orders.cjs               |
| Excluded by settings | js-shop   | `set project.exclude src/notes.*`       | src/billing.mjs, src/catalog.js, src/orders.cjs                              |
| A new file           | js-shop   | `add src/extra.js`                      | src/billing.mjs, src/catalog.js, src/extra.js, src/notes.jsx, src/orders.cjs |

## Choosing languages

`project.languages` limits the analysis to some languages.

[//]: # "decisionTable Core"

| # situation  | workspace | steps                                                | files?                                                         |
| ------------ | --------- | ---------------------------------------------------- | -------------------------------------------------------------- |
| One language | js-shop   | `add src/extra.ts; set project.languages javascript` | src/billing.mjs, src/catalog.js, src/notes.jsx, src/orders.cjs |
| Another one  | js-shop   | `add src/extra.ts; set project.languages typescript` | src/extra.ts                                                   |

[//]: # "decisionTable Core"

| # situation | workspace | steps                          | files?                   |
| ----------- | --------- | ------------------------------ | ------------------------ |
| Python      | mixed     | `set project.languages python` | py/app.py, py/types.pyi  |
| Gleam       | mixed     | `set project.languages gleam`  | gleam/app.gleam          |
| Elixir      | mixed     | `set project.languages elixir` | ex/app.ex, ex/script.exs |

## Recognizing a language

The extension of a file decides its language.

[//]: # "decisionTable Core"

| workspace | subject        | language?  |
| --------- | -------------- | ---------- |
| mixed     | src/main.rs    | rust       |
| mixed     | web/app.ts     | typescript |
| mixed     | web/view.tsx   | typescript |
| mixed     | web/module.mts | typescript |
| mixed     | web/common.cts | typescript |
| mixed     | web/types.d.ts | typescript |
| mixed     | web/legacy.js  | javascript |
| mixed     | web/widget.jsx | javascript |
| mixed     | web/esm.mjs    | javascript |
| mixed     | web/node.cjs   | javascript |

[//]: # "decisionTable Core"

| workspace | subject         | language? |
| --------- | --------------- | --------- |
| mixed     | py/app.py       | python    |
| mixed     | py/types.pyi    | python    |
| mixed     | gleam/app.gleam | gleam     |
| mixed     | ex/app.ex       | elixir    |
| mixed     | ex/script.exs   | elixir    |

Folders a language's tools fill are skipped, as `node_modules` and `target` are:
`__pycache__`, `.venv` and `venv` for Python, `build` for Gleam, and `_build` and `deps`
for Elixir.

[//]: # "decisionTable Core"

| # situation       | workspace  | steps                              | files?                                                                                                       |
| ----------------- | ---------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| A virtual env     | py-shop    | `add .venv/lib/site.py`            | `src/shop/__init__.py, src/shop/billing.py, src/shop/catalog.py, src/shop/notes.py, src/shop/orders.py, tests/test_orders.py` |
| A `venv`          | py-shop    | `add venv/lib/site.py`             | `src/shop/__init__.py, src/shop/billing.py, src/shop/catalog.py, src/shop/notes.py, src/shop/orders.py, tests/test_orders.py` |
| Compiled Python   | py-shop    | `add src/shop/__pycache__/extra.py` | `src/shop/__init__.py, src/shop/billing.py, src/shop/catalog.py, src/shop/notes.py, src/shop/orders.py, tests/test_orders.py` |
| Gleam build       | gleam-shop | `add build/packages/x/src/x.gleam` | src/shop.gleam, src/shop/billing.gleam, src/shop/catalog.gleam, src/shop/notes.gleam, src/shop/orders.gleam, test/orders_test.gleam |
| Elixir build      | ex-shop    | `add _build/dev/lib/x.ex`          | lib/shop/invoice.ex, lib/shop/item.ex, lib/shop/notes.ex, lib/shop/order.ex, lib/shop/priced.ex, mix.exs, test/shop/order_test.exs, test/test_helper.exs |
| Elixir deps       | ex-shop    | `add deps/x/lib/x.ex`              | lib/shop/invoice.ex, lib/shop/item.ex, lib/shop/notes.ex, lib/shop/order.ex, lib/shop/priced.ex, mix.exs, test/shop/order_test.exs, test/test_helper.exs |

## Nothing to analyze

A project with no source code is not an error.

[//]: # "decisionTable Core"

| # situation    | workspace | analyzed? | files? | problems?              |
| -------------- | --------- | --------- | ------ | ---------------------- |
| No source code | docs-only | yes       | none   | No source files found. |
