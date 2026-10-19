# Coding standards

The standards a review applies. Most live beside the code they govern; this file points to them and holds the rules that cut across them.

## Sources

| Source | Covers |
| --- | --- |
| [`docs/agents/rust.md`](docs/agents/rust.md) | Errors, ownership, control flow, tests and lints in Rust |
| [`docs/agents/commits.md`](docs/agents/commits.md) | Commit messages and PR titles: types, scopes, body, footers, vocabulary |
| [`GLOSSARY.md`](GLOSSARY.md) | The domain's terms, and the synonyms to avoid |
| [`docs/adr/`](docs/adr/) | Decisions the code must keep to |
| `crates/platform-android/android/.editorconfig`, `detekt.yml` | Kotlin style (ktlint) and static analysis (detekt) |

## Already checked by CI

Leave these to the tools: clippy with the workspace lints (`unwrap`, `expect`, reasonless `allow`s, small `Copy` types by reference, `let _ =` on a `Result`), rustfmt, ktlint and detekt, the UI's format and lint, a doc comment on every error type and variant (`scripts/check-error-docs`), each crate registered with release-please (`scripts/check-release-config`), and on every PR the commit header, body wrap and footers (`.githooks/commit-msg`), each scope against the files it touches (`scripts/check-commit-scopes`), and avoided glossary acronyms and phrases (`scripts/check-glossary`). Single-word _Avoid_ terms are still the reviewer's.

## Cross-cutting rules

**Tests prove the action.** A test asserts something only the action under test could produce: an answer, an event, a change of state. A value the system already held before the action, such as a `watch` channel's default or an empty list, would pass even if the core had stopped.

**Listener text has one home.** A Device's state and settings labels live in `app/src-tauri/src/labels.rs`, which the window, the Android widget and the tile all read. A rejected command's words live in `WORDS` in `app/src/core.ts`. Each reason gets listener words of its own; developer text (an error's `Display`, an `io::Error`, a plugin's message) goes to the log. Known breaches, to fix rather than copy: `frameLogUnavailable`, `shareFailed` and `appSettingsFailed` in `WORDS` show the shell's message as it is.

**Capitals belong to glossary headwords.** In comments, docs and messages alike, capitalise a word only when it is a `GLOSSARY.md` headword (Device, Attach, Primary Device). Write a type as a link, `` [`Observations`] ``, or as lower-case prose ("the observations").
