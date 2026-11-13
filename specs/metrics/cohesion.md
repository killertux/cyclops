# Cohesion

Back to the [specification index](../README.md).

Cohesion tells whether the members of a type belong together. Cyclops uses LCOM4,
which counts the groups of methods that have nothing to do with each other. Two
methods are in one group if they use the same field, or one calls the other.

A result of 1 is cohesive. Anything higher suggests the type could be split.

- Only instance methods count. Constructors, associated functions and static methods
  do not use the state of an instance.
- In Rust, only inherent methods count. A trait's methods serve the trait, so they
  would make a cohesive type look split.
- A type with no fields or no methods is `n/a`. So is an interface.

## Rust

Each table measures the type `S`.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Snippet"

| # situation         | language | code                                                                                                                                             | lcom? |
| ------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ----- |
| One method          | rust     | `struct S { a: u8 } impl S { fn x(&self) -> u8 { self.a } }`                                                                                     | 1     |
| A shared field      | rust     | `struct S { a: u8 } impl S { fn x(&self) -> u8 { self.a } fn y(&self) -> u8 { self.a } }`                                                        | 1     |
| Separate fields     | rust     | `struct S { a: u8, b: u8 } impl S { fn x(&self) -> u8 { self.a } fn y(&self) -> u8 { self.b } }`                                                 | 2     |
| Joined by a call    | rust     | `struct S { a: u8, b: u8 } impl S { fn x(&self) -> u8 { self.a } fn y(&self) -> u8 { self.b + self.x() } }`                                      | 1     |
| A method on its own | rust     | `struct S { a: u8 } impl S { fn x(&self) -> u8 { self.a } fn y(&self) -> u8 { 1 } }`                                                             | 2     |
| A constructor       | rust     | `struct S { a: u8 } impl S { fn new() -> S { S { a: 1 } } fn x(&self) -> u8 { self.a } }`                                                        | 1     |
| A trait method      | rust     | `struct S { a: u8, b: u8 } trait T { fn y(&self) -> u8; } impl T for S { fn y(&self) -> u8 { self.b } } impl S { fn x(&self) -> u8 { self.a } }` | 1     |
| No methods          | rust     | `struct S { a: u8 }`                                                                                                                             | n/a   |
| No fields           | rust     | `struct S; impl S { fn x(&self) {} }`                                                                                                            | n/a   |
| An enum             | rust     | `enum S { A } impl S { fn x(&self) {} }`                                                                                                         | n/a   |
| A trait             | rust     | `trait S { fn x(&self); }`                                                                                                                       | n/a   |

## TypeScript and JavaScript

[//]: # "decisionTable Snippet"

| # situation                 | language   | code                                                                                                  | lcom? |
| --------------------------- | ---------- | ----------------------------------------------------------------------------------------------------- | ----- |
| A shared field              | typescript | `class S { a = 1; x() { return this.a; } y() { return this.a; } }`                                    | 1     |
| Separate fields             | typescript | `class S { a = 1; b = 2; x() { return this.a; } y() { return this.b; } }`                             | 2     |
| Joined by a call            | typescript | `class S { a = 1; b = 2; x() { return this.a; } y() { return this.b + this.x(); } }`                  | 1     |
| A constructor               | typescript | `class S { a = 1; constructor() { this.a = 2; } x() { return this.a; } }`                             | 1     |
| A static method             | typescript | `class S { a = 1; static make() { return 1; } x() { return this.a; } }`                               | 1     |
| An interface                | typescript | `interface S { x(): number; }`                                                                        | n/a   |
| Separate fields             | javascript | `class S { a = 1; b = 2; x() { return this.a; } y() { return this.b; } }`                             | 2     |
| Fields set in a constructor | javascript | `class S { constructor() { this.a = 1; this.b = 2; } x() { return this.a; } y() { return this.b; } }` | 2     |

## Python

`__init__` is the constructor. Static and class methods do not use an instance.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation      | language | code                                                                                                                                         | lcom? |
| ---------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| A shared field   | python   | `class S:\n    def __init__(self):\n        self.a = 1\n    def x(self):\n        return self.a\n    def y(self):\n        return self.a`     | 1     |
| Separate fields  | python   | `class S:\n    def __init__(self):\n        self.a = 1\n        self.b = 2\n    def x(self):\n        return self.a\n    def y(self):\n        return self.b` | 2 |
| Joined by a call | python   | `class S:\n    a = 1\n    b = 2\n    def x(self):\n        return self.a\n    def y(self):\n        return self.b + self.x()`                 | 1     |
| A static method  | python   | `class S:\n    a = 1\n    @staticmethod\n    def make():\n        return 1\n    def x(self):\n        return self.a`                         | 1     |
| A class method   | python   | `class S:\n    a = 1\n    @classmethod\n    def make(cls):\n        return cls()\n    def x(self):\n        return self.a`                   | 1     |
| A dataclass      | python   | `@dataclass\nclass S:\n    a: int\n    b: int\n    def x(self):\n        return self.a\n    def y(self):\n        return self.b`              | 2     |
| A protocol       | python   | `class S(Protocol):\n    def x(self) -> int: ...`                                                                                            | n/a   |

## Gleam and Elixir

Gleam types and Elixir structs have no methods of their own, so their cohesion is
`n/a`. The methods of an Elixir protocol implementation serve the protocol, as a Rust
trait's do.

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation       | language | code                                                                                          | symbol | lcom? |
| ----------------- | -------- | --------------------------------------------------------------------------------------------- | ------ | ----- |
| A Gleam record    | gleam    | `pub type S { S(a: Int) }\npub fn x(s: S) { s.a }`                                            | S      | n/a   |
| An Elixir struct  | elixir   | `defmodule M do defstruct [:a]; def x(m), do: m.a end`                                        | M::M   | n/a   |
| An implementation | elixir   | `defprotocol P do def x(a) end\ndefmodule M do defstruct [:a]; defimpl P do def x(m), do: m.a end end` | M::M | n/a |

## Across files

A type is measured as a whole, wherever its methods are.

[//]: # "decisionTable Core"

| # situation        | workspace       | subject             | lcom? |
| ------------------ | --------------- | ------------------- | ----- |
| Split in two files | rust-split-impl | split::store::Store | 2     |
| A cohesive type    | rust-shop       | shop::orders::Order | 1     |
| Only trait methods | rust-shop       | shop::catalog::Item | n/a   |
| A cohesive class   | ts-shop         | src/orders::Order   | 1     |

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation          | workspace | subject                | lcom? |
| -------------------- | --------- | ---------------------- | ----- |
| A cohesive Python class | py-shop | src/shop/orders::Order | 1   |
