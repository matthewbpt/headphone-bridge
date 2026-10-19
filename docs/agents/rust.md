# Rust standards

**Production code** is everything outside `#[cfg(test)]` modules and `tests/`. Test code may `unwrap()` and `expect()`. Every `tests/*.rs` file starts with `#![cfg(test)]`, which is how clippy knows its helper functions are test code.

## Errors

Production code returns `Result` and propagates with `?`; it never calls `.unwrap()` or `.expect()`.

- **Library crates** (`crates/*`): a `thiserror` enum, one documented variant per cause the caller can act on, like `CommandError`.
- **Display**: an error's `Display` is developer text for logs; the shell's labels (`labels.rs`) own the text the listener sees.
- **The shell** (`app/src-tauri`): `anyhow` with `.context(...)`, except a `#[tauri::command]`, whose error crosses to the UI and so stays a `Serialize` enum, like `ShareError`.
- **Mutex locks**: `.lock().unwrap_or_else(PoisonError::into_inner)`; our locked data stays valid after a panicking holder.
- **"Checked above" lookups**: make the check yield the value with `if let`, `let ... else` or the `entry` API.

## Ownership

Take `&str`, `&[T]` and `&Path`, and small `Copy` types by value; take owned values only when the function stores them. Clone only to outlive a borrow (a spawned task, a channel send), and clone the cheapest thing that works, such as an `Arc` or an id.

## Control flow

Keep the happy path at the left margin with early returns, `?` and `let ... else`.

## Lints

Before committing, both pass clean (CI denies warnings):

```sh
cargo fmt --all
cargo clippy -p <each crate you touched> --all-targets --locked -- -D warnings
```

Before pushing, `scripts/check` passes: it runs `cargo fmt --check`, clippy with `-D warnings` and the tests over CI's package sets and targets, and the UI checks.

The workspace lints in `Cargo.toml` deny `unwrap`, `expect` and reasonless `allow`s. They also enforce two rules above: small `Copy` types taken by value (`trivially_copy_pass_by_ref`, public items included) and no `let _ =` on a `Result` (`let_underscore_must_use`); discard a `Result` on purpose with `.ok()`. Fix the cause of a warning. When silencing one is right, use `#[expect(lint, reason = "...")]` on the smallest item, wrapped in `cfg_attr` when only some targets trip it. `#[allow(lint, reason = "...")]` is only for a lint that fires in some builds of the same code and not others, such as `dead_code` in shared test support.
