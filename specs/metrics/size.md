# Size

Back to the [specification index](../README.md).

Size is the simplest signal of all. Long functions, long parameter lists and deep
nesting are all hard to work with.

## Lines

Each line of a file is code, a comment, or blank. A line with both code and a comment
is code. With no `symbol`, these columns read the whole file.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Snippet"

| # situation        | language   | code                                        | lines? | comment lines? | blank lines? |
| ------------------ | ---------- | ------------------------------------------- | ------ | -------------- | ------------ |
| One line           | rust       | `fn f() {}`                                 | 1      | 0              | 0            |
| All three          | rust       | `// c\n\nfn f() {\n    // d\n    g();\n}`   | 3      | 2              | 1            |
| A block comment    | rust       | `/* a\n b */\nfn f() {}`                    | 1      | 2              | 0            |
| A doc comment      | rust       | `/// a\nfn f() {}`                          | 1      | 1              | 0            |
| Code and a comment | rust       | `fn f() {} // c`                            | 1      | 0              | 0            |
| All three          | typescript | `// c\n\nfunction f() {\n  // d\n  g();\n}` | 3      | 2              | 1            |
| A block comment    | typescript | `/* a\n b */\nfunction f() {}`              | 1      | 2              | 0            |
| All three          | javascript | `// c\n\nfunction f() {\n  // d\n  g();\n}` | 3      | 2              | 1            |

## Function length

The length of a function is its lines of code, from its signature to its end.

[//]: # "decisionTable Snippet"

| # situation       | language   | code                                   | symbol | lines? |
| ----------------- | ---------- | -------------------------------------- | ------ | ------ |
| One line          | rust       | `fn f() {}`                            | f      | 1      |
| Three lines       | rust       | `fn f() {\n    g();\n}`                | f      | 3      |
| Skips the rest    | rust       | `fn f() {\n\n    // c\n    g();\n}`    | f      | 3      |
| Only the function | rust       | `fn g() {}\nfn f() {\n}\nfn h() {}`    | f      | 2      |
| A method          | typescript | `class C {\n  x() {\n    g();\n  }\n}` | C::x   | 3      |
| An arrow function | typescript | `const f = () => {\n  g();\n};`        | f      | 3      |

## Parameters

`self` and `this` are not counted.

[//]: # "decisionTable Snippet"

| # situation   | language   | code                                         | symbol | parameters? |
| ------------- | ---------- | -------------------------------------------- | ------ | ----------- |
| None          | rust       | `fn f() {}`                                  | f      | 0           |
| Two           | rust       | `fn f(a: u8, b: u8) {}`                      | f      | 2           |
| With `self`   | rust       | `struct S; impl S { fn x(&self, a: u8) {} }` | S::x   | 1           |
| None          | typescript | `function f() {}`                            | f      | 0           |
| A default     | typescript | `function f(a: number, b = 1) {}`            | f      | 2           |
| A rest        | typescript | `function f(a: number, ...b: number[]) {}`   | f      | 2           |
| A `this` type | typescript | `function f(this: Date, a: number) {}`       | f      | 1           |
| An arrow      | javascript | `const f = (a, b) => a;`                     | f      | 2           |

## Nesting

`max nesting?` is the depth of the deepest control structure. A closure adds no depth
of its own.

[//]: # "decisionTable Snippet"

| # situation       | language   | code                                                   | max nesting? |
| ----------------- | ---------- | ------------------------------------------------------ | ------------ |
| None              | rust       | `fn f() {}`                                            | 0            |
| One               | rust       | `fn f(a: bool) { if a {} }`                            | 1            |
| Side by side      | rust       | `fn f(a: bool) { if a {} if a {} }`                    | 1            |
| Two               | rust       | `fn f(a: bool, b: &[u8]) { if a { for _ in b {} } }`   | 2            |
| Through a closure | rust       | `fn f(a: bool) { if a { let g = \|\| { if a {} }; } }` | 2            |
| None              | typescript | `function f() {}`                                      | 0            |
| Two               | typescript | `function f(a: boolean) { if (a) { while (a) {} } }`   | 2            |

## Type size

`members?` counts the fields and methods of a type, wherever its methods are written.

[//]: # "decisionTable Snippet"

| # situation     | language   | code                                                            | symbol | members? |
| --------------- | ---------- | --------------------------------------------------------------- | ------ | -------- |
| A field         | rust       | `struct S { a: u8 }`                                            | S      | 1        |
| Fields, methods | rust       | `struct S { a: u8 } impl S { fn x(&self) {} }`                  | S      | 2        |
| Several `impl`  | rust       | `struct S; impl S { fn x(&self) {} } impl S { fn y(&self) {} }` | S      | 2        |
| A class         | typescript | `class C { a = 1; x() {} }`                                     | C      | 2        |

## Python, Gleam and Elixir

A Python docstring and an Elixir `@doc` or `@moduledoc` are comments, as a Rust doc
comment is. An Elixir function is measured over all its clauses, from the first line of
the first to the last line of the last. Its parameters are its arity, and default
arguments count. Python's `self` and `cls` are not counted, nor the `/` and `*` that
only mark how arguments are passed.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation         | language | code                                                         | lines? | comment lines? | blank lines? |
| ------------------- | -------- | ------------------------------------------------------------ | ------ | -------------- | ------------ |
| All three           | python   | `# c\n\ndef f():\n    # d\n    g()`                          | 2      | 2              | 1            |
| A docstring         | python   | `def f():\n    """a\n    b"""\n    g()`                      | 2      | 2              | 0            |
| Code and a comment  | python   | `g()  # c`                                                   | 1      | 0              | 0            |
| All three           | gleam    | `// c\n\nfn f() {\n  // d\n  g()\n}`                         | 3      | 2              | 1            |
| A doc comment       | gleam    | `/// a\nfn f() { 1 }`                                        | 1      | 1              | 0            |
| A module comment    | gleam    | `//// a\nfn f() { 1 }`                                       | 1      | 1              | 0            |
| All three           | elixir   | `# c\n\ndefmodule M do\n  # d\n  def f, do: 1\nend`          | 3      | 2              | 1            |
| A `@doc`            | elixir   | `defmodule M do\n  @doc """\n  a\n  """\n  def f, do: 1\nend` | 3     | 3              | 0            |
| A `@moduledoc`      | elixir   | `defmodule M do\n  @moduledoc "a"\nend`                      | 2      | 1              | 0            |

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation       | language | code                                                         | symbol | lines? |
| ----------------- | -------- | ------------------------------------------------------------ | ------ | ------ |
| A function        | python   | `def f():\n    g()\n    h()`                                 | f      | 3      |
| A method          | python   | `class C:\n    def x(self):\n        g()`                    | C::x   | 2      |
| A function        | gleam    | `fn f() {\n  g()\n}`                                         | f      | 3      |
| A function        | elixir   | `defmodule M do\n  def f do\n    g()\n  end\nend`            | M::f/0 | 3      |
| Several clauses   | elixir   | `defmodule M do\n  def f(0), do: 0\n\n  def f(_), do: 1\nend` | M::f/1 | 2     |

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation        | language | code                                                      | symbol | parameters? |
| ------------------ | -------- | --------------------------------------------------------- | ------ | ----------- |
| None               | python   | `def f():\n    pass`                                      | f      | 0           |
| With `self`        | python   | `class C:\n    def x(self, a):\n        pass`            | C::x   | 1           |
| With `cls`         | python   | `class C:\n    @classmethod\n    def x(cls, a):\n        pass` | C::x | 1        |
| `*args`, `**kwargs` | python  | `def f(a, *b, **c):\n    pass`                            | f      | 3           |
| Markers            | python   | `def f(a, /, b, *, c):\n    pass`                         | f      | 3           |
| Labels             | gleam    | `fn f(a x: Int, b y: Int) { x + y }`                      | f      | 2           |
| Its arity          | elixir   | `defmodule M do def f(a, b), do: a end`                   | M::f/2 | 2           |
| A default          | elixir   | `defmodule M do def f(a, b \\ 1), do: a + b end`          | M::f/2 | 2           |

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation | language | code                                                                     | symbol | max nesting? |
| ----------- | -------- | ------------------------------------------------------------------------ | ------ | ------------ |
| Two         | python   | `def f(a):\n    if a:\n        for x in a:\n            pass`             | f      | 2            |
| Two         | gleam    | `fn f(a, b) { case a { True -> case b { _ -> 1 } False -> 0 } }`         | f      | 2            |
| Two         | elixir   | `defmodule M do def f(a, b) do case a do 1 -> if b do 1 end; _ -> 0 end end end` | M::f/2 | 2   |

A Python class counts the fields set on `self` too. A Gleam type counts its fields, and
an Elixir struct its fields and the methods of its protocol implementations.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation       | language | code                                                                                  | symbol | members? |
| ----------------- | -------- | ------------------------------------------------------------------------------------- | ------ | -------- |
| A class           | python   | `class C:\n    a = 1\n    def x(self):\n        pass`                                  | C      | 2        |
| Fields on `self`  | python   | `class C:\n    def __init__(self):\n        self.a = 1\n        self.b = 2`          | C      | 3        |
| A record          | gleam    | `pub type S { S(a: Int, b: Int) }`                                                    | S      | 2        |
| A struct          | elixir   | `defmodule M do defstruct [:a, :b] end`                                               | M::M   | 2        |
| An implementation | elixir   | `defprotocol P do def x(a) end\ndefmodule M do defstruct [:a]; defimpl P do def x(m), do: m.a end end` | M::M | 2 |
