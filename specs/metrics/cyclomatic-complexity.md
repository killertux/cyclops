# Cyclomatic Complexity

Back to the [specification index](../README.md).

Cyclomatic complexity counts the independent paths through a function. It starts at
1, and a branch with N ways out adds N - 1. Each table measures the function `f`.

Shorthand operators are branches too, because they can skip code: `&&`, `||`, `??`,
`?.`, and the `?` operator in Rust.

## Rust

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Snippet"

| # situation             | language | code                                                        | cyclomatic? |
| ----------------------- | -------- | ----------------------------------------------------------- | ----------- |
| Straight code           | rust     | `fn f() {}`                                                 | 1           |
| `if`                    | rust     | `fn f(a: bool) { if a {} }`                                 | 2           |
| `else` adds none        | rust     | `fn f(a: bool) { if a {} else {} }`                         | 2           |
| `else if`               | rust     | `fn f(a: u8) { if a == 1 {} else if a == 2 {} else {} }`    | 3           |
| `if let`                | rust     | `fn f(a: Option<u8>) { if let Some(_) = a {} }`             | 2           |
| `let else`              | rust     | `fn f(a: Option<u8>) { let Some(_) = a else { return; }; }` | 2           |
| `for`                   | rust     | `fn f(n: u8) { for _ in 0..n {} }`                          | 2           |
| `while`                 | rust     | `fn f(a: bool) { while a {} }`                              | 2           |
| `while let`             | rust     | `fn f(a: Option<u8>) { while let Some(_) = a {} }`          | 2           |
| `loop` has no exit test | rust     | `fn f() { loop {} }`                                        | 1           |
| One arm                 | rust     | `fn f(a: u8) -> u8 { match a { _ => 1 } }`                  | 1           |
| Two arms                | rust     | `fn f(a: bool) -> u8 { match a { true => 1, false => 0 } }` | 2           |
| Three arms              | rust     | `fn f(a: u8) -> u8 { match a { 1 => 1, 2 => 2, _ => 3 } }`  | 3           |
| Patterns in one arm     | rust     | `fn f(a: u8) -> u8 { match a { 1 \| 2 => 1, _ => 0 } }`     | 2           |
| A match guard           | rust     | `fn f(a: u8) -> u8 { match a { x if x > 1 => 1, _ => 0 } }` | 3           |
| `&&`                    | rust     | `fn f(a: bool, b: bool) -> bool { a && b }`                 | 2           |
| `\|\|`                  | rust     | `fn f(a: bool, b: bool) -> bool { a \|\| b }`               | 2           |
| Both                    | rust     | `fn f(a: bool, b: bool, c: bool) -> bool { a && b \|\| c }` | 3           |
| `?`                     | rust     | `fn f(a: Option<u8>) -> Option<u8> { let b = a?; Some(b) }` | 2           |

## TypeScript

[//]: # "decisionTable Snippet"

| # situation         | language   | code                                                                                     | cyclomatic? |
| ------------------- | ---------- | ---------------------------------------------------------------------------------------- | ----------- |
| Straight code       | typescript | `function f() {}`                                                                        | 1           |
| `if`                | typescript | `function f(a: boolean) { if (a) {} }`                                                   | 2           |
| `else if`           | typescript | `function f(a: number) { if (a === 1) {} else if (a === 2) {} else {} }`                 | 3           |
| `for`               | typescript | `function f() { for (let i = 0; i < 2; i++) {} }`                                        | 2           |
| `for of`            | typescript | `function f(a: number[]) { for (const x of a) {} }`                                      | 2           |
| `for in`            | typescript | `function f(a: object) { for (const x in a) {} }`                                        | 2           |
| `while`             | typescript | `function f(a: boolean) { while (a) {} }`                                                | 2           |
| `do while`          | typescript | `function f(a: boolean) { do {} while (a); }`                                            | 2           |
| With a `default`    | typescript | `function f(a: number) { switch (a) { case 1: break; case 2: break; default: break; } }` | 3           |
| Without a `default` | typescript | `function f(a: number) { switch (a) { case 1: break; case 2: break; } }`                 | 3           |
| A ternary           | typescript | `function f(a: boolean) { return a ? 1 : 2; }`                                           | 2           |
| `&&`                | typescript | `function f(a: boolean, b: boolean) { return a && b; }`                                  | 2           |
| `\|\|`              | typescript | `function f(a: boolean, b: boolean) { return a \|\| b; }`                                | 2           |
| `??`                | typescript | `function f(a?: number) { return a ?? 1; }`                                              | 2           |
| `?.`                | typescript | `function f(a?: Date) { return a?.getTime(); }`                                          | 2           |
| `catch`             | typescript | `function f() { try {} catch {} }`                                                       | 2           |
| `finally` adds none | typescript | `function f() { try {} finally {} }`                                                     | 1           |

## JavaScript

JavaScript follows the same rules.

[//]: # "decisionTable Snippet"

| # situation   | language   | code                                   | cyclomatic? |
| ------------- | ---------- | -------------------------------------- | ----------- |
| Straight code | javascript | `function f() {}`                      | 1           |
| `if`          | javascript | `function f(a) { if (a) {} }`          | 2           |
| A ternary     | javascript | `function f(a) { return a ? 1 : 2; }`  | 2           |
| `catch`       | javascript | `function f() { try {} catch (e) {} }` | 2           |

## Functions inside functions

A closure counts towards the function around it, because it is not a
[symbol](../model/symbols.md#closures). A named function inside another is measured on
its own.

[//]: # "decisionTable Snippet"

| # situation     | language   | code                                                       | cyclomatic? |
| --------------- | ---------- | ---------------------------------------------------------- | ----------- |
| A Rust closure  | rust       | `fn f(a: bool) { let g = \|\| { if a {} }; g(); }`         | 2           |
| A named Rust fn | rust       | `fn f() { fn g(a: bool) { if a {} } }`                     | 1           |
| A TS arrow      | typescript | `function f(a: boolean) { [1].map(() => { if (a) {} }); }` | 2           |
| A named TS fn   | typescript | `function f() { function g(a: boolean) { if (a) {} } }`    | 1           |
