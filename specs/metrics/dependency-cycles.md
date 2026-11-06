# Dependency Cycles

Back to the [specification index](../README.md).

A dependency cycle is a group of modules that all depend on each other, directly or
not, so none of them can be understood or changed alone. The cycles samples have the
same imports: `a` and `b` import each other, `c`, `d` and `e` form a ring, and `f`
imports `a`.

`cycles?` lists each cycle, separated by semicolons.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation | workspace   | cycles?                                               |
| ----------- | ----------- | ----------------------------------------------------- |
| Rust        | rust-cycles | cycles::a, cycles::b; cycles::c, cycles::d, cycles::e |
| TypeScript  | ts-cycles   | src/a, src/b; src/c, src/d, src/e                     |
| No cycles   | rust-shop   | none                                                  |

## Being part of a cycle

`in cycle` is true for each member of a cycle. A module that depends on a cycle is not
part of it.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation        | workspace   | subject   | in cycle? | cycle size? |
| ------------------ | ----------- | --------- | --------- | ----------- |
| A pair             | rust-cycles | cycles::a | true      | 2           |
| A ring             | rust-cycles | cycles::d | true      | 3           |
| Depends on a cycle | rust-cycles | cycles::f | false     | n/a         |
| A pair             | ts-cycles   | src/b     | true      | 2           |
| Depends on a cycle | ts-cycles   | src/f     | false     | n/a         |
