# Parse Failures

Back to the [specification index](../README.md).

One bad file never stops an analysis. A file that cannot be fully understood is
listed in `problems?`, and everything else is analyzed as usual.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation     | workspace | steps                    | analyzed? | problems?                                          |
| --------------- | --------- | ------------------------ | --------- | -------------------------------------------------- |
| No problems     | rust-shop |                          | yes       | none                                               |
| Syntax errors   | broken    |                          | yes       | src/bad.rs: syntax error, web/bad.ts: syntax error |
| Not text        | rust-shop | `add binary src/blob.rs` | yes       | src/blob.rs: not valid UTF-8                       |
| Unreadable file | rust-shop | `lock src/notes.rs`      | yes       | src/notes.rs: cannot be read                       |

[//]: # "decisionTable Core"

| # situation    | workspace    | analyzed? | problems?                                                                   |
| -------------- | ------------ | --------- | --------------------------------------------------------------------------- |
| Syntax errors  | broken-langs | yes       | ex/bad.ex: syntax error, gleam/bad.gleam: syntax error, py/bad.py: syntax error |

## What survives

Code around a syntax error still counts, as far as it can be understood.

[//]: # "decisionTable Core"

| # situation         | workspace | subject            | found? |
| ------------------- | --------- | ------------------ | ------ |
| A healthy Rust file | broken    | broken::good::fine | yes    |
| Before a Rust error | broken    | broken::bad::kept  | yes    |
| A healthy TS file   | broken    | web/good::fine     | yes    |
| Before a TS error   | broken    | web/bad::kept      | yes    |

[//]: # "decisionTable Core"

| # situation           | workspace    | subject         | found? |
| --------------------- | ------------ | --------------- | ------ |
| A healthy Python file | broken-langs | py/good::fine   | yes    |
| Before a Python error | broken-langs | py/bad::kept    | yes    |
| A healthy Gleam file  | broken-langs | gleam/good::fine | yes   |
| Before a Gleam error  | broken-langs | gleam/bad::kept | yes    |
| A healthy Elixir file | broken-langs | Good::fine/0    | yes    |
| Before an Elixir error | broken-langs | Bad::kept/0    | yes    |
