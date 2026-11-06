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
[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

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

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

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

## Across files

A type is measured as a whole, wherever its methods are.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation        | workspace       | subject             | lcom? |
| ------------------ | --------------- | ------------------- | ----- |
| Split in two files | rust-split-impl | split::store::Store | 2     |
| A cohesive type    | rust-shop       | shop::orders::Order | 1     |
| Only trait methods | rust-shop       | shop::catalog::Item | n/a   |
| A cohesive class   | ts-shop         | src/orders::Order   | 1     |
