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

[//]: # "decisionTable Snippet"

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

## Python

A conditional expression counts as a ternary does. Each `for` and `if` of a
comprehension adds 1, with no nesting, since a comprehension reads on one line.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation          | language | code                                                                                          | cognitive? |
| -------------------- | -------- | --------------------------------------------------------------------------------------------- | ---------- |
| Straight code        | python   | `def f():\n    pass`                                                                          | 0          |
| `if`                 | python   | `def f(a):\n    if a:\n        pass`                                                          | 1          |
| `else`               | python   | `def f(a):\n    if a:\n        pass\n    else:\n        pass`                                 | 2          |
| `elif`               | python   | `def f(a):\n    if a == 1:\n        pass\n    elif a == 2:\n        pass\n    else:\n        pass` | 3     |
| Nested once          | python   | `def f(a, b):\n    if a:\n        if b:\n            pass`                                    | 3          |
| A whole `match`      | python   | `def f(a):\n    match a:\n        case 1:\n            pass\n        case _:\n            pass` | 1         |
| A conditional        | python   | `def f(a):\n    return 1 if a else 2`                                                         | 1          |
| `except`             | python   | `def f():\n    try:\n        pass\n    except ValueError:\n        pass`                      | 1          |
| A nested `except`    | python   | `def f(a):\n    if a:\n        try:\n            pass\n        except ValueError:\n            pass` | 3     |
| A run of `and`       | python   | `def f(a, b, c):\n    return a and b and c`                                                   | 1          |
| A change of operator | python   | `def f(a, b, c):\n    return a and b or c`                                                    | 2          |
| A comprehension      | python   | `def f(a):\n    return [x for x in a if x]`                                                    | 2          |
| Recursion            | python   | `def f(n):\n    return f(n)`                                                                  | 1          |
| A lambda             | python   | `def f(a):\n    g = lambda: 1 if a else 2`                                                    | 2          |
| `with` adds none     | python   | `def f(a):\n    with a:\n        pass`                                                         | 0          |

## Gleam

A whole `case` counts once, as a `match` does. Gleam loops by recursion, and each
recursive call adds 1. `use` adds no nesting, since the code after it reads as straight
code.

[//]: # "decisionTable Snippet"

| # situation          | language | code                                                                         | cognitive? |
| -------------------- | -------- | ---------------------------------------------------------------------------- | ---------- |
| Straight code        | gleam    | `fn f() { Nil }`                                                             | 0          |
| A whole `case`       | gleam    | `fn f(a) { case a { 1 -> 1 2 -> 2 _ -> 3 } }`                                | 1          |
| A nested `case`      | gleam    | `fn f(a, b) { case a { True -> case b { True -> 1 False -> 2 } False -> 0 } }` | 3        |
| One operator         | gleam    | `fn f(a, b) { a && b }`                                                      | 1          |
| A run of one         | gleam    | `fn f(a, b, c) { a && b && c }`                                              | 1          |
| A change of operator | gleam    | `fn f(a, b, c) { a && b \|\| c }`                                            | 2          |
| Recursion            | gleam    | `fn f(n) { f(n) }`                                                           | 1          |
| An anonymous fn      | gleam    | `fn f(a) { list.map([1], fn(x) { case a { True -> x False -> 0 } }) }`       | 2          |
| After a `use`        | gleam    | `fn f(a) {\n  use x <- result.try(a)\n  case x { 1 -> Ok(1) _ -> Ok(0) }\n}` | 1          |

## Elixir

`if` and `unless` add 1, and `else` adds 1 more. A whole `case`, `cond`, `with` or
`receive` counts once, and so does `rescue`. The clauses of a function add nothing,
because each one reads as a small function of its own. A pipe adds nothing.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation          | language | code                                                                           | symbol | cognitive? |
| -------------------- | -------- | ------------------------------------------------------------------------------ | ------ | ---------- |
| Straight code        | elixir   | `defmodule M do def f, do: 1 end`                                              | M::f/0 | 0          |
| `if`                 | elixir   | `defmodule M do def f(a) do if a do 1 end end end`                             | M::f/1 | 1          |
| `else`               | elixir   | `defmodule M do def f(a) do if a do 1 else 2 end end end`                      | M::f/1 | 2          |
| `unless`             | elixir   | `defmodule M do def f(a) do unless a do 1 end end end`                         | M::f/1 | 1          |
| A whole `case`       | elixir   | `defmodule M do def f(a) do case a do 1 -> 1; 2 -> 2; _ -> 3 end end end`      | M::f/1 | 1          |
| A whole `cond`       | elixir   | `defmodule M do def f(a, b) do cond do a -> 1; b -> 2; true -> 3 end end end`  | M::f/2 | 1          |
| A whole `with`       | elixir   | `defmodule M do def f(a) do with {:ok, x} <- a, {:ok, y} <- x do y end end end` | M::f/1 | 1         |
| Nested in a `case`   | elixir   | `defmodule M do def f(a, b) do case a do 1 -> if b do 1 end; _ -> 0 end end end` | M::f/2 | 3       |
| Function clauses     | elixir   | `defmodule M do def f(0), do: 0; def f(_), do: 1 end`                          | M::f/1 | 0          |
| `rescue`             | elixir   | `defmodule M do def f(a) do try do a.() rescue _ -> 1 end end end`             | M::f/1 | 1          |
| A run of one         | elixir   | `defmodule M do def f(a, b, c), do: a and b and c end`                         | M::f/3 | 1          |
| A change of operator | elixir   | `defmodule M do def f(a, b, c), do: a && b \|\| c end`                         | M::f/3 | 2          |
| Recursion            | elixir   | `defmodule M do def f(n), do: f(n) end`                                        | M::f/1 | 1          |
| A pipe               | elixir   | `defmodule M do def f(a), do: a \|> g() \|> h() end`                           | M::f/1 | 0          |
| An `fn`              | elixir   | `defmodule M do def f(a) do Enum.map([1], fn x -> if a do x end end) end end`  | M::f/1 | 2          |
