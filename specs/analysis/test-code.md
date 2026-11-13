# Test Code

Back to the [specification index](../README.md).

Test code is left out of measurement by default, because test duplication and
fixtures would otherwise drown out real findings. It can be switched on through the
`tests.include` [setting](../project/configuration.md), or the `--tests` and
`--no-tests` flags.

## Recognizing test code

A file's path can mark all of it as test code. In Rust, code inside a file can be
test code too. `role?` can be read whether or not test code is left out.

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation         | workspace | steps                                  | subject            | role?      |
| ------------------- | --------- | -------------------------------------- | ------------------ | ---------- |
| Rust source         | rust-shop |                                        | src/orders.rs      | production |
| Rust `tests/`       | rust-shop | `add tests/extra.rs`                   | tests/extra.rs     | test       |
| Rust `benches/`     | rust-shop | `add benches/speed.rs`                 | benches/speed.rs   | test       |
| TS source           | ts-shop   |                                        | src/orders.ts      | production |
| `.test.` file       | ts-shop   | `add src/a.test.ts`                    | src/a.test.ts      | test       |
| `.spec.` file       | ts-shop   | `add src/a.spec.tsx`                   | src/a.spec.tsx     | test       |
| `__tests__/` folder | ts-shop   | `add src/__tests__/a.ts`               | `src/__tests__/a.ts` | test       |
| `tests/` folder     | ts-shop   | `add tests/a.ts`                       | tests/a.ts         | test       |
| `test/` folder      | ts-shop   | `add test/a.ts`                        | test/a.ts          | test       |
| JS `.test.` file    | js-shop   | `add src/a.test.js`                    | src/a.test.js      | test       |
| Extra paths         | ts-shop   | `add e2e/a.ts; set tests.paths e2e/**` | e2e/a.ts           | test       |

[//]: # "decisionTable Snippet"

| # situation      | language | code                                        | symbol        | role?      |
| ---------------- | -------- | ------------------------------------------- | ------------- | ---------- |
| A plain function | rust     | `fn f() {}`                                 | f             | production |
| A `#[test]`      | rust     | `#[test] fn f() {}`                         | f             | test       |
| A test module    | rust     | `#[cfg(test)] mod tests { fn helper() {} }` | tests::helper | test       |

In Python, `test_*.py`, `*_test.py`, `conftest.py` and a `tests/` folder are test code,
as pytest finds them. In Gleam and Elixir, the `test/` folder is.

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation       | workspace  | steps                         | subject                 | role?      |
| ----------------- | ---------- | ----------------------------- | ----------------------- | ---------- |
| Python source     | py-shop    |                               | src/shop/orders.py      | production |
| A `test_` file    | py-shop    | `add src/shop/test_extra.py`  | src/shop/test_extra.py  | test       |
| A `_test` file    | py-shop    | `add src/shop/extra_test.py`  | src/shop/extra_test.py  | test       |
| `conftest.py`     | py-shop    | `add conftest.py`             | conftest.py             | test       |
| `tests/` folder   | py-shop    | `add tests/extra.py`          | tests/extra.py          | test       |
| Gleam source      | gleam-shop |                               | src/shop/orders.gleam   | production |
| Gleam `test/`     | gleam-shop | `add test/extra_test.gleam`   | test/extra_test.gleam   | test       |
| Elixir source     | ex-shop    |                               | lib/shop/order.ex       | production |
| Elixir `test/`    | ex-shop    | `add test/extra_test.exs`     | test/extra_test.exs     | test       |

## Leaving it out

Excluded test code is hidden everywhere, and it contributes to no metric.

[//]: # "decisionTable Core"

| # situation     | workspace | steps                    | subject                        | found? |
| --------------- | --------- | ------------------------ | ------------------------------ | ------ |
| Production code | rust-shop |                          | shop::notes::greeting          | yes    |
| A test module   | rust-shop |                          | shop::notes::tests::says_hello | no     |
| Switched on     | rust-shop | `set tests.include true` | shop::notes::tests::says_hello | yes    |
| A test file     | ts-shop   |                          | src/orders.test::startsEmpty   | no     |
| Switched on     | ts-shop   | `set tests.include true` | src/orders.test::startsEmpty   | yes    |

Its dependencies, lines and clones do not count either.

[//]: # "decisionTable Core"

| # situation    | workspace | steps                    | subject      | afferent? | instability? |
| -------------- | --------- | ------------------------ | ------------ | --------- | ------------ |
| Rust, left out | rust-shop |                          | shop::orders | 0         | 1.00         |
| Rust, included | rust-shop | `set tests.include true` | shop::orders | 1         | 0.67         |
| TS, left out   | ts-shop   |                          | src/orders   | 0         | 1.00         |
| TS, included   | ts-shop   | `set tests.include true` | src/orders   | 1         | 0.67         |

[//]: # "decisionTable Core"

| # situation | workspace | steps                    | subject | lines? |
| ----------- | --------- | ------------------------ | ------- | ------ |
| Left out    | rust-shop |                          | shop    | 55     |
| Included    | rust-shop | `set tests.include true` | shop    | 69     |

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation        | workspace  | steps                    | subject                              | found? |
| ------------------ | ---------- | ------------------------ | ------------------------------------ | ------ |
| A Python test      | py-shop    |                          | tests/test_orders::test_starts_empty | no     |
| Switched on        | py-shop    | `set tests.include true` | tests/test_orders::test_starts_empty | yes    |
| A Gleam test       | gleam-shop |                          | test/orders_test::starts_empty_test  | no     |
| Switched on        | gleam-shop | `set tests.include true` | test/orders_test::starts_empty_test  | yes    |
| An Elixir test     | ex-shop    |                          | Shop.OrderTest                       | no     |
| Switched on        | ex-shop    | `set tests.include true` | Shop.OrderTest                       | yes    |

[//]: # "decisionTable Core -- snooze until 2099-12-31"

| # situation       | workspace  | steps                    | subject         | afferent? | instability? |
| ----------------- | ---------- | ------------------------ | --------------- | --------- | ------------ |
| Python, left out  | py-shop    |                          | src/shop/orders | 0         | 1.00         |
| Python, included  | py-shop    | `set tests.include true` | src/shop/orders | 1         | 0.67         |
| Gleam, left out   | gleam-shop |                          | src/shop/orders | 0         | 1.00         |
| Gleam, included   | gleam-shop | `set tests.include true` | src/shop/orders | 1         | 0.67         |
| Elixir, left out  | ex-shop    |                          | Shop.Order      | 0         | 1.00         |
| Elixir, included  | ex-shop    | `set tests.include true` | Shop.Order      | 1         | 0.67         |

Switching test code on or off [re-parses nothing](caching.md).
