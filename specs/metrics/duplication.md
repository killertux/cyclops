# Duplication

Back to the [specification index](../README.md).

Duplication finds repeated code, which is what the DRY principle warns against. Names
and literal values are ignored, so a renamed copy is still a copy. Pieces shorter than
`metrics.duplication.min_tokens` are ignored, and copies only match within one
language family.

## Clone groups

`clones?` lists each group of files that share a copy, separated by semicolons. In
the [`clones`](../../samples/clones) sample, `first.rs` and `second.rs` are the same,
`renamed.rs` has other names and values, and `partial.rs` has a copy and something
else. `web/` has a TypeScript version.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation    | workspace | steps                                     | clones?                                                                                                    |
| -------------- | --------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| The copies     | clones    |                                           | src/first.rs, src/partial.rs, src/renamed.rs, src/second.rs; web/renamed.ts, web/total.ts                  |
| With test code | clones    | `set tests.include true`                  | src/first.rs, src/partial.rs, src/renamed.rs, src/second.rs, tests/copied.rs; web/renamed.ts, web/total.ts |
| A high minimum | clones    | `set metrics.duplication.min_tokens 1000` | none                                                                                                       |
| No copies      | rust-shop |                                           | none                                                                                                       |

Rust and TypeScript copies of the same logic are not reported together, as the first
row shows.

## Measuring a file

`duplication?` is the percentage of the code lines of a file or module that are part
of a copy.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation       | workspace | subject        | duplication? |
| ----------------- | --------- | -------------- | ------------ |
| All of it         | clones    | src/first.rs   | 100.00       |
| A renamed copy    | clones    | src/renamed.rs | 100.00       |
| Half of it        | clones    | src/partial.rs | 50.00        |
| None of it        | clones    | src/unique.rs  | 0.00         |
| A TypeScript file | clones    | web/total.ts   | 100.00       |
| A folder          | clones    | web            | 100.00       |

## Reading a group

`copies?` lists where the copies of the subject are, with their lines.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation  | workspace | subject       | copies?                                                      |
| ------------ | --------- | ------------- | ------------------------------------------------------------ |
| One group    | clones    | web/total.ts  | web/renamed.ts:1-12                                          |
| A larger one | clones    | src/first.rs  | src/partial.rs:1-12, src/renamed.rs:1-12, src/second.rs:1-12 |
| No copies    | clones    | src/unique.rs | none                                                         |
