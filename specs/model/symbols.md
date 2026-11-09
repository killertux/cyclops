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

## Closures

[//]: # "decisionTable Snippet"

| # situation    | language   | code                                           | symbol | found? |
| -------------- | ---------- | ---------------------------------------------- | ------ | ------ |
| A Rust closure | rust       | `fn f() { let g = \|\| 1; }`                   | f::g   | no     |
| A TS callback  | typescript | `function f(a: number[]) { a.map((x) => x); }` | f::x   | no     |

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
