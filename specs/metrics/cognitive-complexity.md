# Cognitive Complexity

Back to the [specification index](../README.md).

Cognitive complexity estimates how hard a function is for a person to follow. It uses
the rules of [SonarSource's Cognitive Complexity](https://www.sonarsource.com/docs/CognitiveComplexity.pdf).
Unlike [cyclomatic complexity](cyclomatic-complexity.md), it starts at 0, and it
punishes nesting.

- Each `if`, `else if`, `else`, ternary, loop, `match`, `switch` and `catch` adds 1.
- A structure also adds 1 for each level it is nested, except `else` and `else if`.
- A whole `match` or `switch` counts once, however many arms it has.
- A run of the same logical operator adds 1. Changing operator adds 1 again.
- Recursion adds 1, and jumping to a label adds 1.
- Closures add no points, but what is inside them is one level deeper.

## Rust

Each table measures the function `f`. The `?` operator adds nothing, because it reads
as straight code.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Snippet"

| # situation          | language | code                                                                      | cognitive? |
| -------------------- | -------- | ------------------------------------------------------------------------- | ---------- |
| Straight code        | rust     | `fn f() {}`                                                               | 0          |
| `if`                 | rust     | `fn f(a: bool) { if a {} }`                                               | 1          |
| `else`               | rust     | `fn f(a: bool) { if a {} else {} }`                                       | 2          |
| `else if`            | rust     | `fn f(a: u8) { if a == 1 {} else if a == 2 {} else {} }`                  | 3          |
| Nested once          | rust     | `fn f(a: bool, b: bool) { if a { if b {} } }`                             | 3          |
| Nested twice         | rust     | `fn f(a: bool, b: bool, c: &[u8]) { for _ in c { if a { while b {} } } }` | 6          |
| `else` when nested   | rust     | `fn f(a: bool, b: bool) { if a { if b {} else {} } }`                     | 4          |
| `loop`               | rust     | `fn f() { loop {} }`                                                      | 1          |
| A whole `match`      | rust     | `fn f(a: u8) -> u8 { match a { 1 => 1, 2 => 2, _ => 3 } }`                | 1          |
| A nested `match`     | rust     | `fn f(a: bool, b: u8) { if a { match b { _ => {} } } }`                   | 3          |
| One operator         | rust     | `fn f(a: bool, b: bool) -> bool { a && b }`                               | 1          |
| A run of one         | rust     | `fn f(a: bool, b: bool, c: bool) -> bool { a && b && c }`                 | 1          |
| A change of operator | rust     | `fn f(a: bool, b: bool, c: bool) -> bool { a && b \|\| c }`               | 2          |
| `?`                  | rust     | `fn f(a: Option<u8>) -> Option<u8> { let b = a?; Some(b) }`               | 0          |
| Recursion            | rust     | `fn f(n: u8) -> u8 { f(n) }`                                              | 1          |
| A plain `break`      | rust     | `fn f() { loop { break; } }`                                              | 1          |
| A labeled `break`    | rust     | `fn f() { 'a: loop { break 'a; } }`                                       | 2          |
| A closure            | rust     | `fn f(a: bool) { let g = \|\| { if a {} }; g(); }`                        | 2          |
| A named function     | rust     | `fn f() { fn g(a: bool) { if a {} } }`                                    | 0          |

## TypeScript and JavaScript

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation       | language   | code                                                                      | cognitive? |
| ----------------- | ---------- | ------------------------------------------------------------------------- | ---------- |
| Straight code     | typescript | `function f() {}`                                                         | 0          |
| `if`              | typescript | `function f(a: boolean) { if (a) {} }`                                    | 1          |
| Nested once       | typescript | `function f(a: boolean, b: boolean) { if (a) { if (b) {} } }`             | 3          |
| A whole `switch`  | typescript | `function f(a: number) { switch (a) { case 1: break; default: break; } }` | 1          |
| A ternary         | typescript | `function f(a: boolean) { return a ? 1 : 2; }`                            | 1          |
| A nested ternary  | typescript | `function f(a: boolean, b: boolean) { return a ? (b ? 1 : 2) : 3; }`      | 3          |
| `catch`           | typescript | `function f() { try {} catch {} }`                                        | 1          |
| A nested `catch`  | typescript | `function f(a: boolean) { if (a) { try {} catch {} } }`                   | 3          |
| `??`              | typescript | `function f(a?: number) { return a ?? 1; }`                               | 1          |
| `?.`              | typescript | `function f(a?: Date) { return a?.getTime(); }`                           | 0          |
| A labeled `break` | typescript | `function f() { outer: for (;;) { break outer; } }`                       | 2          |
| An arrow          | typescript | `function f(a: boolean) { [1].map(() => { if (a) {} }); }`                | 2          |
| Recursion         | javascript | `function f(n) { return f(n); }`                                          | 1          |
| Nested once       | javascript | `function f(a, b) { if (a) { if (b) {} } }`                               | 3          |
