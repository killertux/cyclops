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

## What survives

Code around a syntax error still counts, as far as it can be understood.

[//]: # "decisionTable Core"

| # situation         | workspace | subject            | found? |
| ------------------- | --------- | ------------------ | ------ |
| A healthy Rust file | broken    | broken::good::fine | yes    |
| Before a Rust error | broken    | broken::bad::kept  | yes    |
| A healthy TS file   | broken    | web/good::fine     | yes    |
| Before a TS error   | broken    | web/bad::kept      | yes    |
