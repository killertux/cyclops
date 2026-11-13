# Symbols

Back to the [specification index](../README.md).

Cyclops describes every language with the same small set of symbols, so that metrics,
filters and diagrams work the same way for all of them.

| Level   | Kinds                                           |
| ------- | ----------------------------------------------- |
| Project | `project`                                       |
| Package | `package`                                       |
| Module  | `module`                                        |
| Type    | `struct`, `class`, `enum`, `interface`, `alias` |
| Member  | `function`, `method`, `field`, `constant`       |

## Rust

A trait is an `interface`, so that it means the same as in TypeScript. Methods belong
to their type, whichever `impl` block they are in.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Snippet"

| # situation         | language | code                                                                 | symbol | kind?     | parent? |
| ------------------- | -------- | -------------------------------------------------------------------- | ------ | --------- | ------- |
| A struct            | rust     | `struct S;`                                                          | S      | struct    | none    |
| An enum             | rust     | `enum E { A }`                                                       | E      | enum      | none    |
| A trait             | rust     | `trait T {}`                                                         | T      | interface | none    |
| A type alias        | rust     | `type A = u8;`                                                       | A      | alias     | none    |
| A function          | rust     | `fn f() {}`                                                          | f      | function  | none    |
| A constant          | rust     | `const C: u8 = 1;`                                                   | C      | constant  | none    |
| A field             | rust     | `struct S { a: u8 }`                                                 | S::a   | field     | S       |
| An inherent method  | rust     | `struct S; impl S { fn x(&self) {} }`                                | S::x   | method    | S       |
| A trait method      | rust     | `struct S; trait T { fn x(&self); } impl T for S { fn x(&self) {} }` | S::x   | method    | S       |
| A method of a trait | rust     | `trait T { fn x(&self); }`                                           | T::x   | method    | T       |
| An inline module    | rust     | `mod m { fn g() {} }`                                                | m      | module    | none    |
| Inside a module     | rust     | `mod m { fn g() {} }`                                                | m::g   | function  | m       |
| A nested function   | rust     | `fn f() { fn g() {} }`                                               | f::g   | function  | f       |

A method and a field of one type may share a name, as a getter does. The method keeps
the plain name, and the field is written with `#field` after it.

[//]: # "decisionTable Snippet"

| # situation   | language | code                                                         | symbol     | kind?  | parent? |
| ------------- | -------- | ------------------------------------------------------------ | ---------- | ------ | ------- |
| A getter      | rust     | `struct S { x: u8 } impl S { fn x(&self) -> u8 { self.x } }` | S::x       | method | S       |
| Its field     | rust     | `struct S { x: u8 } impl S { fn x(&self) -> u8 { self.x } }` | S::x#field | field  | S       |
| A field alone | rust     | `struct S { a: u8 }`                                         | S::a       | field  | S       |

## TypeScript and JavaScript

A function stored in a variable is a function. A closure or an arrow function passed
along is not a symbol, and counts as part of the function around it.

[//]: # "decisionTable Snippet"

| # situation       | language   | code                        | symbol | kind?     | parent? |
| ----------------- | ---------- | --------------------------- | ------ | --------- | ------- |
| A class           | typescript | `class C {}`                | C      | class     | none    |
| An interface      | typescript | `interface I {}`            | I      | interface | none    |
| An enum           | typescript | `enum E { A }`              | E      | enum      | none    |
| A type alias      | typescript | `type A = number;`          | A      | alias     | none    |
| A function        | typescript | `function f() {}`           | f      | function  | none    |
| An arrow function | typescript | `const f = () => {};`       | f      | function  | none    |
| A constant        | typescript | `const c = 1;`              | c      | constant  | none    |
| A field           | typescript | `class C { x = 1; }`        | C::x   | field     | C       |
| A method          | typescript | `class C { x() {} }`        | C::x   | method    | C       |
| A class           | javascript | `class C {}`                | C      | class     | none    |
| A method          | javascript | `class C { x() {} }`        | C::x   | method    | C       |
| A function        | javascript | `function f() {}`           | f      | function  | none    |
| A function value  | javascript | `const f = function () {};` | f      | function  | none    |

## Python

A class is a `class`, unless it is a protocol or an abstract base, which is an
`interface`, or an enum. A name assigned at the top of a module is a `constant`, since
Python has no constants of its own and such a name plays their part. A field is a name
declared in the class body, or assigned on `self` in any of its methods.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation         | language | code                                                          | symbol | kind?     | parent? |
| ------------------- | -------- | ------------------------------------------------------------- | ------ | --------- | ------- |
| A class             | python   | `class C:\n    pass`                                          | C      | class     | none    |
| A protocol          | python   | `from typing import Protocol\nclass P(Protocol):\n    pass`   | P      | interface | none    |
| An abstract base    | python   | `from abc import ABC\nclass P(ABC):\n    pass`                | P      | interface | none    |
| An enum             | python   | `from enum import Enum\nclass E(Enum):\n    A = 1`            | E      | enum      | none    |
| A type alias        | python   | `type A = int`                                                | A      | alias     | none    |
| An annotated alias  | python   | `from typing import TypeAlias\nA: TypeAlias = int`            | A      | alias     | none    |
| A function          | python   | `def f():\n    pass`                                          | f      | function  | none    |
| A lambda in a name  | python   | `f = lambda: 1`                                               | f      | function  | none    |
| A constant          | python   | `LIMIT = 1`                                                   | LIMIT  | constant  | none    |
| Any module name     | python   | `count = 0`                                                   | count  | constant  | none    |
| A method            | python   | `class C:\n    def x(self):\n        pass`                    | C::x   | method    | C       |
| A class attribute   | python   | `class C:\n    x = 1`                                         | C::x   | field     | C       |
| An annotated field  | python   | `class C:\n    x: int`                                        | C::x   | field     | C       |
| Set on `self`       | python   | `class C:\n    def __init__(self):\n        self.x = 1`       | C::x   | field     | C       |
| Set in a method     | python   | `class C:\n    def reset(self):\n        self.x = 0`          | C::x   | field     | C       |
| A nested class      | python   | `class C:\n    class D:\n        pass`                        | C::D   | class     | C       |
| A nested function   | python   | `def f():\n    def g():\n        pass`                        | f::g   | function  | f       |

## Gleam

Gleam has no classes and no methods. A custom type with one constructor is a `struct`,
and its labelled fields are its fields. A custom type with several constructors is an
`enum`. Every function belongs to its module.

[//]: # "decisionTable Snippet"

| # situation          | language | code                                          | symbol | kind?    | parent? |
| -------------------- | -------- | --------------------------------------------- | ------ | -------- | ------- |
| A record type        | gleam    | `pub type S { S(a: Int) }`                    | S      | struct   | none    |
| Its field            | gleam    | `pub type S { S(a: Int) }`                    | S::a   | field    | S       |
| Several constructors | gleam    | `pub type E { A B }`                          | E      | enum     | none    |
| A type alias         | gleam    | `pub type A = Int`                            | A      | alias    | none    |
| A function           | gleam    | `pub fn f() { 1 }`                            | f      | function | none    |
| A constant           | gleam    | `pub const c = 1`                             | c      | constant | none    |
| An external function | gleam    | `@external(erlang, "m", "f")\npub fn f() -> Int` | f   | function | none    |

## Elixir

A symbol is named after its module, and a function after its name and arity, as
Elixir writes them: `Shop.Order::total/1`. Every clause of one name and arity is one
function, and a function with default arguments is named after its largest arity.

A module is a `module`. A dotted name puts it inside the module of its prefix, as a
folder holds a file, so `Shop.Order` is inside `Shop` even when nothing defines `Shop`.
A module with `defstruct` or `defexception` also holds a `struct` named after the last
part of its name, and the struct holds the fields. A protocol holds an `interface` in
the same way. A protocol's implementation adds its functions to the struct as methods,
wherever it is written, as a Rust trait `impl` does. A module attribute that holds a
value, such as `@limit 10`, is a `constant`. Attributes Elixir itself reads, such as
`@doc`, `@spec` or `@behaviour`, are not.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation           | language | code                                                                                        | symbol    | kind?     | parent? |
| --------------------- | -------- | ------------------------------------------------------------------------------------------- | --------- | --------- | ------- |
| A module              | elixir   | `defmodule M do end`                                                                        | M         | module    | none    |
| A nested module       | elixir   | `defmodule M do defmodule N do end end`                                                     | M.N       | module    | M       |
| A dotted name         | elixir   | `defmodule A.B do end`                                                                      | A.B       | module    | A       |
| Its prefix            | elixir   | `defmodule A.B do end`                                                                      | A         | module    | none    |
| A function            | elixir   | `defmodule M do def f(a), do: a end`                                                        | M::f/1    | function  | M       |
| Several clauses       | elixir   | `defmodule M do def f(0), do: 0; def f(n), do: n end`                                       | M::f/1    | function  | M       |
| Another arity         | elixir   | `defmodule M do def f(a), do: a; def f(a, b), do: b end`                                    | M::f/2    | function  | M       |
| A default argument    | elixir   | `defmodule M do def f(a, b \\ 1), do: a + b end`                                            | M::f/2    | function  | M       |
| A macro               | elixir   | `defmodule M do defmacro m(a), do: a end`                                                   | M::m/1    | function  | M       |
| A guard               | elixir   | `defmodule M do defguard g(a) when a > 1 end`                                               | M::g/1    | function  | M       |
| A delegate            | elixir   | `defmodule M do defdelegate f(a), to: N end`                                                | M::f/1    | function  | M       |
| An attribute          | elixir   | `defmodule M do @limit 10 end`                                                              | M::@limit | constant  | M       |
| A struct              | elixir   | `defmodule M do defstruct [:a] end`                                                         | M::M      | struct    | M       |
| Its field             | elixir   | `defmodule M do defstruct [:a] end`                                                         | M::M::a   | field     | M::M    |
| Fields with defaults  | elixir   | `defmodule M do defstruct a: 1 end`                                                         | M::M::a   | field     | M::M    |
| An exception          | elixir   | `defmodule M do defexception [:message] end`                                                | M::M      | struct    | M       |
| A protocol            | elixir   | `defprotocol P do def x(a) end`                                                             | P::P      | interface | P       |
| Its function          | elixir   | `defprotocol P do def x(a) end`                                                             | P::P::x/1 | method    | P::P    |
| An implementation     | elixir   | `defprotocol P do def x(a) end\ndefmodule M do defstruct [:a]; defimpl P do def x(m), do: m.a end end` | M::M::x/1 | method | M::M |
| Implemented elsewhere | elixir   | `defprotocol P do def x(a) end\ndefmodule M do defstruct [:a] end\ndefimpl P, for: M do def x(m), do: m.a end` | M::M::x/1 | method | M::M |

## Closures

[//]: # "decisionTable Snippet"

| # situation    | language   | code                                           | symbol | found? |
| -------------- | ---------- | ---------------------------------------------- | ------ | ------ |
| A Rust closure | rust       | `fn f() { let g = \|\| 1; }`                   | f::g   | no     |
| A TS callback  | typescript | `function f(a: number[]) { a.map((x) => x); }` | f::x   | no     |

A Python lambda, a Gleam anonymous function and an Elixir `fn` are closures too, and so
are an Elixir attribute that Elixir reads and a Python name inside a function.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation           | language | code                                                         | symbol        | found? |
| --------------------- | -------- | ------------------------------------------------------------ | ------------- | ------ |
| A Python lambda       | python   | `def f(a):\n    return map(lambda x: x, a)`                  | f::x          | no     |
| A local Python name   | python   | `def f():\n    total = 0`                                    | f::total      | no     |
| A Gleam function      | gleam    | `fn f(a) { list.map(a, fn(x) { x }) }`                       | f::x          | no     |
| An Elixir `fn`        | elixir   | `defmodule M do def f(a), do: Enum.map(a, fn x -> x end) end` | M::f/1::x     | no     |
| Elixir documentation  | elixir   | `defmodule M do @moduledoc "a" end`                          | M::@moduledoc | no     |

## Visibility

[//]: # "decisionTable Snippet"

| # situation       | language   | code                           | symbol | visibility? |
| ----------------- | ---------- | ------------------------------ | ------ | ----------- |
| Rust `pub`        | rust       | `pub fn f() {}`                | f      | public      |
| Rust default      | rust       | `fn f() {}`                    | f      | private     |
| Rust `pub(crate)` | rust       | `pub(crate) fn f() {}`         | f      | restricted  |
| TS `export`       | typescript | `export function f() {}`       | f      | public      |
| TS default        | typescript | `function f() {}`              | f      | private     |
| TS member         | typescript | `class C { x() {} }`           | C::x   | public      |
| TS `private`      | typescript | `class C { private x() {} }`   | C::x   | private     |
| TS `protected`    | typescript | `class C { protected x() {} }` | C::x   | restricted  |
| JS `#` member     | javascript | `class C { #x() {} }`          | C::#x  | private     |

A Python name that starts with `_` is private, but a name like `__init__`, which
Python itself calls, is public. Gleam's `pub opaque type` is public, and its fields are
private. An Elixir attribute only exists inside its module.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation         | language | code                                           | symbol      | visibility? |
| ------------------- | -------- | ---------------------------------------------- | ----------- | ----------- |
| Python default      | python   | `def f():\n    pass`                           | f           | public      |
| Python `_`          | python   | `def _f():\n    pass`                          | _f          | private     |
| Python `__` member  | python   | `class C:\n    def __x(self):\n        pass`   | C::__x      | private     |
| Python dunder       | python   | `class C:\n    def __init__(self):\n        pass` | C::__init__ | public   |
| Python `_` field    | python   | `class C:\n    def __init__(self):\n        self._x = 1` | C::_x | private  |
| Gleam `pub`         | gleam    | `pub fn f() { 1 }`                             | f           | public      |
| Gleam default       | gleam    | `fn f() { 1 }`                                 | f           | private     |
| Gleam opaque type   | gleam    | `pub opaque type S { S(a: Int) }`              | S           | public      |
| Its field           | gleam    | `pub opaque type S { S(a: Int) }`              | S::a        | private     |
| Elixir `def`        | elixir   | `defmodule M do def f, do: 1 end`              | M::f/0      | public      |
| Elixir `defp`       | elixir   | `defmodule M do defp f, do: 1 end`             | M::f/0      | private     |
| Elixir `defmacrop`  | elixir   | `defmodule M do defmacrop m, do: 1 end`        | M::m/0      | private     |
| Elixir attribute    | elixir   | `defmodule M do @limit 10 end`                 | M::@limit   | private     |
| Elixir module       | elixir   | `defmodule M do end`                           | M           | public      |

## The hierarchy

Every symbol sits inside another, from the top down. A project of several packages is
a symbol of its own, named after its folder, and holds them. A project of one package
is that package, which then has no parent. In TypeScript and JavaScript a file is a
module, and a folder is the module that holds them. An `index` file is the module of
its folder.

[//]: # "decisionTable Core"

| # situation        | workspace       | subject                    | kind?   | parent?             |
| ------------------ | --------------- | -------------------------- | ------- | ------------------- |
| A Rust package     | rust-shop       | shop                       | package | none                |
| A Rust module      | rust-shop       | shop::orders               | module  | shop                |
| A Rust type        | rust-shop       | shop::orders::Order        | struct  | shop::orders        |
| A Rust method      | rust-shop       | shop::orders::Order::total | method  | shop::orders::Order |
| A method elsewhere | rust-split-impl | split::store::Store::label | method  | split::store::Store |
| A TS package       | ts-shop         | shop                       | package | none                |
| A TS file          | ts-shop         | src/orders                 | module  | src                 |
| A TS folder        | ts-shop         | src                        | module  | shop                |
| An index file      | ts-resolution   | src/barrel                 | module  | src                 |
| A file in a folder | ts-resolution   | src/barrel/thing           | module  | src/barrel          |

Children are listed in alphabetical order.

[//]: # "decisionTable Core"

| # situation | workspace | subject               | children?                                                                           |
| ----------- | --------- | --------------------- | ----------------------------------------------------------------------------------- |
| A package   | rust-shop | shop                  | shop::billing, shop::catalog, shop::notes, shop::orders                             |
| A module    | rust-shop | shop::orders          | shop::orders::Order                                                                 |
| A type      | rust-shop | shop::orders::Order   | shop::orders::Order::add, shop::orders::Order::invoices, shop::orders::Order::total |
| No children | rust-shop | shop::notes::greeting | none                                                                                |

A workspace of several packages has its project above them.

[//]: # "decisionTable Core"

| # situation      | workspace   | subject     | kind?   | parent?     |
| ---------------- | ----------- | ----------- | ------- | ----------- |
| A workspace      | rust-crates | rust-crates | project | none        |
| A package in it  | rust-crates | engine      | package | rust-crates |
| A module in that | rust-crates | engine::big | module  | engine      |

[//]: # "decisionTable Core"

| # situation | workspace   | subject     | children?                            |
| ----------- | ----------- | ----------- | ------------------------------------ |
| A project   | rust-crates | rust-crates | c01, c02, c03, c04, c05, c06, engine |

## The hierarchy of the other languages

Python and Gleam follow TypeScript: a file is a module, and a folder is the module that
holds them. Python's `__init__.py` is the module of its folder, as `index.ts` is. A
Gleam file next to a folder of the same name, such as `src/shop.gleam` next to
`src/shop/`, is the module of that folder. A package takes its name from
`pyproject.toml`, `gleam.toml` or `mix.exs`, or else from its folder.

An Elixir module is named after itself, not its file, and sits inside the module of its
prefix, up to the package.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation              | workspace  | subject                          | kind?     | parent?                |
| ------------------------ | ---------- | -------------------------------- | --------- | ---------------------- |
| A Python package         | py-shop    | shop                             | package   | none                   |
| A source folder          | py-shop    | src                              | module    | shop                   |
| A folder with `__init__` | py-shop    | src/shop                         | module    | src                    |
| A Python file            | py-shop    | src/shop/orders                  | module    | src/shop               |
| A Python class           | py-shop    | src/shop/orders::Order           | class     | src/shop/orders        |
| A Python method          | py-shop    | src/shop/orders::Order::total    | method    | src/shop/orders::Order |
| A Python field           | py-shop    | src/shop/orders::Order::invoices | field     | src/shop/orders::Order |
| A Gleam package          | gleam-shop | shop                             | package   | none                   |
| A file beside its folder | gleam-shop | src/shop                         | module    | src                    |
| A Gleam file             | gleam-shop | src/shop/orders                  | module    | src/shop               |
| A Gleam type             | gleam-shop | src/shop/orders::Order           | struct    | src/shop/orders        |
| A Gleam field            | gleam-shop | src/shop/orders::Order::invoices | field     | src/shop/orders::Order |
| A Gleam function         | gleam-shop | src/shop/orders::total           | function  | src/shop/orders        |
| An Elixir package        | ex-shop    | shop                             | package   | none                   |
| A prefix                 | ex-shop    | Shop                             | module    | shop                   |
| An Elixir module         | ex-shop    | Shop.Order                       | module    | Shop                   |
| An Elixir struct         | ex-shop    | Shop.Order::Order                | struct    | Shop.Order             |
| An Elixir function       | ex-shop    | Shop.Order::total/1              | function  | Shop.Order             |
| An Elixir protocol       | ex-shop    | Shop.Priced::Priced              | interface | Shop.Priced            |
| A protocol's method      | ex-shop    | Shop.Item::Item::price/1         | method    | Shop.Item::Item        |

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation             | workspace  | subject           | children?                                                                         |
| ----------------------- | ---------- | ----------------- | --------------------------------------------------------------------------------- |
| A Python package folder | py-shop    | src/shop          | src/shop/billing, src/shop/catalog, src/shop/notes, src/shop/orders               |
| A Gleam module folder   | gleam-shop | src/shop          | src/shop/billing, src/shop/catalog, src/shop/notes, src/shop/orders, src/shop::main |
| An Elixir prefix        | ex-shop    | Shop              | Shop.Invoice, Shop.Item, Shop.MixProject, Shop.Notes, Shop.Order, Shop.Priced     |
| An Elixir module        | ex-shop    | Shop.Order        | Shop.Order::Order, Shop.Order::add/2, Shop.Order::new/0, Shop.Order::total/1      |
| An Elixir struct        | ex-shop    | Shop.Order::Order | Shop.Order::Order::invoices                                                       |
