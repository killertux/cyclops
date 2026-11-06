# Size

Back to the [specification index](../README.md).

Size is the simplest signal of all. Long functions, long parameter lists and deep
nesting are all hard to work with.

## Lines

Each line of a file is code, a comment, or blank. A line with both code and a comment
is code. With no `symbol`, these columns read the whole file.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

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

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

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

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

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

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

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

[//]: # "decisionTable Snippet -- snooze until 2099-12-31"

| # situation     | language   | code                                                            | symbol | members? |
| --------------- | ---------- | --------------------------------------------------------------- | ------ | -------- |
| A field         | rust       | `struct S { a: u8 }`                                            | S      | 1        |
| Fields, methods | rust       | `struct S { a: u8 } impl S { fn x(&self) {} }`                  | S      | 2        |
| Several `impl`  | rust       | `struct S; impl S { fn x(&self) {} } impl S { fn y(&self) {} }` | S      | 2        |
| A class         | typescript | `class C { a = 1; x() {} }`                                     | C      | 2        |
