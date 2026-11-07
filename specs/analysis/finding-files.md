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

## Nothing to analyze

A project with no source code is not an error.

[//]: # "decisionTable Core"

| # situation    | workspace | analyzed? | files? | problems?              |
| -------------- | --------- | --------- | ------ | ---------------------- |
| No source code | docs-only | yes       | none   | No source files found. |
