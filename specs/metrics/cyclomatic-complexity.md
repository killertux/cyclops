# Cyclomatic Complexity

Back to the [specification index](../README.md).

Cyclomatic complexity counts the independent paths through a function. It starts at
1, and a branch with N ways out adds N - 1. Each table measures the function `f`.

Shorthand operators are branches too, because they can skip code: `&&`, `||`, `??`,
`?.`, the `?` operator in Rust, and `and` and `or` in Python and Elixir.

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

## Python

A comprehension is a loop, and its `if` is a branch.

[//]: # "decisionTable Snippet"

| # situation            | language | code                                                                                                   | cyclomatic? |
| ---------------------- | -------- | ------------------------------------------------------------------------------------------------------ | ----------- |
| Straight code          | python   | `def f():\n    pass`                                                                                   | 1           |
| `if`                   | python   | `def f(a):\n    if a:\n        pass`                                                                   | 2           |
| `else` adds none       | python   | `def f(a):\n    if a:\n        pass\n    else:\n        pass`                                          | 2           |
| `elif`                 | python   | `def f(a):\n    if a == 1:\n        pass\n    elif a == 2:\n        pass`                               | 3           |
| `for`                  | python   | `def f(a):\n    for x in a:\n        pass`                                                             | 2           |
| `while`                | python   | `def f(a):\n    while a:\n        pass`                                                                 | 2           |
| Three cases            | python   | `def f(a):\n    match a:\n        case 1:\n            pass\n        case 2:\n            pass\n        case _:\n            pass` | 3 |
| Patterns in one case   | python   | `def f(a):\n    match a:\n        case 1 \| 2:\n            pass\n        case _:\n            pass`    | 2           |
| A case guard           | python   | `def f(a):\n    match a:\n        case x if x > 1:\n            pass\n        case _:\n            pass` | 3           |
| A conditional          | python   | `def f(a):\n    return 1 if a else 2`                                                                  | 2           |
| `and`                  | python   | `def f(a, b):\n    return a and b`                                                                      | 2           |
| `or`                   | python   | `def f(a, b):\n    return a or b`                                                                       | 2           |
| Both                   | python   | `def f(a, b, c):\n    return a and b or c`                                                              | 3           |
| `except`               | python   | `def f():\n    try:\n        pass\n    except ValueError:\n        pass`                               | 2           |
| Two `except`           | python   | `def f():\n    try:\n        pass\n    except ValueError:\n        pass\n    except KeyError:\n        pass` | 3     |
| `finally` adds none    | python   | `def f():\n    try:\n        pass\n    finally:\n        pass`                                          | 1           |
| A comprehension        | python   | `def f(a):\n    return [x for x in a]`                                                                  | 2           |
| Its `if`               | python   | `def f(a):\n    return [x for x in a if x]`                                                             | 3           |
| `with` adds none       | python   | `def f(a):\n    with a:\n        pass`                                                                  | 1           |

## Gleam

Gleam branches with `case` alone, so a `case` counts as a Rust `match` does. `use` and
`let assert` add nothing: `use` only passes the rest of the function along, and a
failed `let assert` stops the program.

[//]: # "decisionTable Snippet"

| # situation             | language | code                                               | cyclomatic? |
| ----------------------- | -------- | -------------------------------------------------- | ----------- |
| Straight code           | gleam    | `fn f() { Nil }`                                   | 1           |
| One clause              | gleam    | `fn f(a) { case a { _ -> 1 } }`                    | 1           |
| Two clauses             | gleam    | `fn f(a) { case a { True -> 1 False -> 0 } }`      | 2           |
| Three clauses           | gleam    | `fn f(a) { case a { 1 -> 1 2 -> 2 _ -> 3 } }`      | 3           |
| Patterns in one clause  | gleam    | `fn f(a) { case a { 1 \| 2 -> 1 _ -> 0 } }`        | 2           |
| A guard                 | gleam    | `fn f(a) { case a { x if x > 1 -> 1 _ -> 0 } }`    | 3           |
| `&&`                    | gleam    | `fn f(a, b) { a && b }`                            | 2           |
| `\|\|`                  | gleam    | `fn f(a, b) { a \|\| b }`                          | 2           |
| `use` adds none         | gleam    | `fn f(a) {\n  use x <- result.try(a)\n  Ok(x)\n}`  | 1           |
| `let assert` adds none  | gleam    | `fn f(a) {\n  let assert Ok(x) = a\n  x\n}`        | 1           |

## Elixir

A function of several clauses chooses one, as a `case` does, so each clause after the
first adds 1, and so does each guard. `if` and `unless` add 1, and `case`, `cond` and
`receive` count as a Rust `match` does. Each step of a `with` can fail, and adds 1. A
pipe adds nothing.

[//]: # "decisionTable Snippet"

| # situation             | language | code                                                                     | symbol | cyclomatic? |
| ----------------------- | -------- | ------------------------------------------------------------------------ | ------ | ----------- |
| Straight code           | elixir   | `defmodule M do def f, do: 1 end`                                        | M::f/0 | 1           |
| `if`                    | elixir   | `defmodule M do def f(a) do if a do 1 end end end`                       | M::f/1 | 2           |
| `else` adds none        | elixir   | `defmodule M do def f(a) do if a do 1 else 2 end end end`                | M::f/1 | 2           |
| `unless`                | elixir   | `defmodule M do def f(a) do unless a do 1 end end end`                   | M::f/1 | 2           |
| Two clauses             | elixir   | `defmodule M do def f(a) do case a do 1 -> 1; _ -> 2 end end end`        | M::f/1 | 2           |
| `cond`                  | elixir   | `defmodule M do def f(a, b) do cond do a -> 1; b -> 2; true -> 3 end end end` | M::f/2 | 3      |
| A guard in a `case`     | elixir   | `defmodule M do def f(a) do case a do x when x > 1 -> 1; _ -> 0 end end end` | M::f/1 | 3       |
| Function clauses        | elixir   | `defmodule M do def f(0), do: 0; def f(_), do: 1 end`                    | M::f/1 | 2           |
| A guard on a function   | elixir   | `defmodule M do def f(a) when a > 1, do: a end`                          | M::f/1 | 2           |
| `with`                  | elixir   | `defmodule M do def f(a) do with {:ok, x} <- a do x end end end`         | M::f/1 | 2           |
| Two steps of a `with`   | elixir   | `defmodule M do def f(a) do with {:ok, x} <- a, {:ok, y} <- x do y end end end` | M::f/1 | 3    |
| `and`                   | elixir   | `defmodule M do def f(a, b), do: a and b end`                            | M::f/2 | 2           |
| `\|\|`                  | elixir   | `defmodule M do def f(a, b), do: a \|\| b end`                           | M::f/2 | 2           |
| `rescue`                | elixir   | `defmodule M do def f(a) do try do a.() rescue _ -> 1 end end end`       | M::f/1 | 2           |
| `receive`               | elixir   | `defmodule M do def f do receive do :a -> 1; :b -> 2 end end end`        | M::f/0 | 2           |
| A pipe adds none        | elixir   | `defmodule M do def f(a), do: a \|> g() end`                             | M::f/1 | 1           |

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

[//]: # "decisionTable Snippet"

| # situation          | language | code                                                                        | symbol | cyclomatic? |
| -------------------- | -------- | --------------------------------------------------------------------------- | ------ | ----------- |
| A Python lambda      | python   | `def f(a):\n    g = lambda: 1 if a else 2`                                  | f      | 2           |
| A named Python def   | python   | `def f():\n    def g(a):\n        if a:\n            pass`                  | f      | 1           |
| A Gleam function     | gleam    | `fn f(a) { list.map([1], fn(x) { case a { True -> x False -> 0 } }) }`      | f      | 2           |
| An Elixir `fn`       | elixir   | `defmodule M do def f(a) do Enum.map([1], fn x -> if a do x end end) end end` | M::f/1 | 2         |
| Clauses of an `fn`   | elixir   | `defmodule M do def f(a) do Enum.map(a, fn 0 -> 0; _ -> 1 end) end end`     | M::f/1 | 2           |
