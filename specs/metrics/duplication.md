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
[//]: # "decisionTable Core"

| # situation    | workspace | steps                                     | clones?                                                                                                    |
| -------------- | --------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| The copies     | clones    |                                           | src/first.rs, src/partial.rs, src/renamed.rs, src/second.rs; web/renamed.ts, web/total.ts                  |
| With test code | clones    | `set tests.include true`                  | src/first.rs, src/partial.rs, src/renamed.rs, src/second.rs, tests/copied.rs; web/renamed.ts, web/total.ts |
| A high minimum | clones    | `set metrics.duplication.min_tokens 1000` | none                                                                                                       |
| No copies      | rust-shop |                                           | none                                                                                                       |

Rust and TypeScript copies of the same logic are not reported together, as the first
row shows.

## Measuring a file or a function

`duplication?` is the percentage of the code lines of a function, file or module that
are part of a copy. A type is measured over all its lines, like a module.

[//]: # "decisionTable Core"

| # situation       | workspace | subject        | duplication? |
| ----------------- | --------- | -------------- | ------------ |
| All of it         | clones    | src/first.rs   | 100.00       |
| A renamed copy    | clones    | src/renamed.rs | 100.00       |
| Half of it        | clones    | src/partial.rs | 50.00        |
| None of it        | clones    | src/unique.rs  | 0.00         |
| A TypeScript file | clones    | web/total.ts   | 100.00       |
| A folder          | clones    | web            | 100.00       |

[//]: # "decisionTable Core"

| # situation           | workspace | subject                   | duplication? |
| --------------------- | --------- | ------------------------- | ------------ |
| A copied function     | clones    | clones::first::total      | 100.00       |
| The copy in a file    | clones    | clones::partial::total    | 100.00       |
| The rest of that file | clones    | clones::partial::describe | 0.00         |
| A renamed copy        | clones    | clones::renamed::amount   | 100.00       |
| No copies             | clones    | clones::unique::average   | 0.00         |
| A TypeScript function | clones    | web/total::total          | 100.00       |

The Overview and the ranking still rank duplication by file.

## Reading a group

`copies?` lists where the copies of the subject are, with their lines.

[//]: # "decisionTable Core"

| # situation  | workspace | subject       | copies?                                                      |
| ------------ | --------- | ------------- | ------------------------------------------------------------ |
| One group    | clones    | web/total.ts  | web/renamed.ts:1-12                                          |
| A larger one | clones    | src/first.rs  | src/partial.rs:1-12, src/renamed.rs:1-12, src/second.rs:1-12 |
| No copies    | clones    | src/unique.rs | none                                                         |
