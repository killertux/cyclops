# Agent Guidelines

Rules for any agent working in this repository. "Human approval" means explicit
approval from a human in the current conversation or task. If you need approval,
stop and ask before making the change.

Start with [IDEA.md](IDEA.md) for what Cyclops is, [PROPOSAL.md](PROPOSAL.md) for its
architecture, and [specs/README.md](specs/README.md) for how it behaves.

## Before committing

Always run these first:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Before finishing a change

All of these must pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
temoc -c temoc.toml
```

Report any failure honestly. Do not describe work as done while a check fails.

## Linting and formatting

- Always obey `cargo clippy` and `cargo fmt`. Fix the code, not the tool.
- Do not override the linter or formatter without human approval. This includes:
  - `#[allow(...)]`, `#[expect(...)]`, and their crate-level `#![...]` forms
  - `#[rustfmt::skip]`
  - Adding or changing `clippy.toml`, `rustfmt.toml`, or the `[lints]` tables in
    `Cargo.toml`
  - Weakening lints through `RUSTFLAGS`, `--cap-lints`, or changed CLI flags
- When asking for approval, name the lint, explain why the code should not
  change, and propose the narrowest possible scope.
- Approved lint overrides use `#[expect(..., reason = "...")]`, not `#[allow]`.
  Any other approved override gets a comment stating the reason.

## Error handling

- Non-test code must not panic. Return `Result` or `Option` and propagate
  failures with `?`.
- Clippy denies `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`,
  `unreachable!`, and direct indexing or slicing. Use `get()`, pattern matching,
  or a proper error instead.
- Lints do not catch every panic, such as `assert!` or division by zero. Avoid
  those in non-test code too.
- Do not silently discard errors with `let _ =` or `.ok()`. Handle or propagate
  them.

## Architecture

- Follow the crate boundaries in [PROPOSAL.md](PROPOSAL.md). Dependencies point
  inward, and the domain depends on nothing internal.
- Interfaces hold no analysis logic. If a future GUI or web UI would need the same
  behavior, it belongs in the core.
- If a task needs a change to the architecture, stop and ask a human.

## Dependencies

- Do not add a dependency without human approval, unless it is listed in
  [PROPOSAL.md](PROPOSAL.md). This applies to `[dependencies]`,
  `[dev-dependencies]`, and `[build-dependencies]`.
- When asking for approval, name the crate, say why it is needed, and say why
  the standard library or an existing dependency is not enough.

## Specification

- The `specs/` folder is the source of truth for how the software behaves. Read
  the relevant parts before implementing or changing behavior, and obey them.
- Never edit `specs/` or `samples/` without human approval. `samples/` holds the
  workspaces the specification runs against.
- Work spec first. A new behavior starts as a table in `specs/`, approved by a
  human and marked `-- snooze until 2099-12-31`. Then the code makes it pass.
- A snoozed table is specified but not built yet, and removing those snoozes is the
  goal. When a table passes, remove its snooze in the same change. That one edit is
  already approved. `grep -rn 'snooze until 2099' specs` lists what is left, and
  `temoc -c temoc.toml -s` shows why each table fails.
- Do not add a snooze to a table that has lost one without human approval.
- If the spec is ambiguous, incomplete, or conflicts with the code or the task,
  stop and ask a human. Do not guess, and do not change the spec to fit the code.
- See [how the specification works](specs/writing-specs.md) for the fixtures, steps
  and samples.

## Testing

- Add tests that validate the software. New behavior ships with tests, and bug
  fixes ship with a regression test that fails without the fix.
- Test the behavior the spec requires, not just the implementation details.
- Do not delete, weaken, or `#[ignore]` a test to get a passing run without
  human approval.
