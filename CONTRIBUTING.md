# Contributing to Resolve

Thanks for wanting to help. Resolve is deliberately small, so the best contributions make the
existing features faster, clearer or more pleasant rather than adding new ones.

## Before you start

- For bugs, open an issue with steps to reproduce, what you expected and what happened.
- For new features, open an issue first to discuss it. Resolve focuses on five things: tasks,
  Discipline Points, the dashboard, statistics and history. A feature that doesn't improve one
  of those will probably be declined.

## Development

```bash
cargo run                 # debug build
cargo test                # unit tests
RESOLVE_DATA_DIR=/tmp/resolve-dev cargo run   # use a throwaway database
```

Before opening a pull request, make sure these pass (CI runs the same checks):

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Guidelines

- Keep business rules in `src/core`. UI code renders state and emits actions; it doesn't decide
  what those actions mean.
- Schema changes go in a **new** entry at the end of `MIGRATIONS` in
  `src/persistence/sqlite.rs`. Never edit an existing migration: people already have databases
  created with it.
- Add a test for any change to `core` or `persistence`.
- Prefer the standard library and existing dependencies. A new crate needs a good reason.
- Keep animations subtle and cheap: only request repaints while something is actually moving.
- Write commit messages that explain *why*, not just *what*.

By contributing you agree that your work is released under the [MIT License](LICENSE).
