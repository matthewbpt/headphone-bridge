# Commits

Every commit message follows [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/). The `commit-msg` hook in `.githooks/` checks the header locally, and the `Commits` workflow runs the same hook on every commit in a pull request.

Pull requests are squash-merged: the PR title becomes the header of the one commit that lands on `main`, and the PR's commit messages become its body. The title follows the same format, and the `Commits` workflow checks it too. release-please reads that commit to choose the next version, so title a PR for its most significant change: `feat` if any commit is a `feat`, with `!` if any commit breaks an interface.

## Setup

Run once per clone (worktrees share it):

```sh
git config core.hooksPath .githooks
```

## Format

```
<type>(<scope>)!: <description>

<body>

<footers>
```

- **Header**: at most 72 characters. The `(<scope>)` and `!` are optional.
- **Description**: imperative mood, starting with a lower-case verb ("add", "fix", "let"), no trailing period. Glossary terms keep their capitals: `let a Device settle`, not `let a device settle`.
- **Body**: optional, after one blank line. Explain why the change is needed and what it trades off; the diff already shows what changed. Wrap at 72 characters.
- **Footers**: after one blank line, one `Token: value` per line.

## Types

| Type       | Use for                                                       |
| ---------- | ------------------------------------------------------------- |
| `feat`     | A new behaviour the listener or a developer can use           |
| `fix`      | A bug fix                                                     |
| `refactor` | A code change that neither adds behaviour nor fixes a bug     |
| `perf`     | A change that makes something faster or lighter               |
| `test`     | Adding or correcting tests and fixtures only                  |
| `docs`     | Documentation only: README, ADRs, `GLOSSARY.md`, agent docs   |
| `build`    | Cargo, npm, Tauri bundling and other build configuration      |
| `ci`       | GitHub Actions workflows                                      |
| `chore`    | Upkeep that fits nowhere else, e.g. `.gitignore`              |
| `style`    | Formatting only (`cargo fmt`, whitespace), no behaviour change |
| `revert`   | Reverting an earlier commit                                   |

## Scopes

A scope names the one area the commit touches. When a commit spans several areas, or none of these fits, omit the scope rather than invent a vague one like `misc` or `app`.

| Scope      | Area                                                               |
| ---------- | ------------------------------------------------------------------ |
| `core`     | `crates/core`: the device model, commands and updates              |
| `samsung`  | `crates/core/src/samsung`: the Galaxy Buds protocol                |
| `sony`     | `crates/core/src/sony`: the WH-1000XM5 protocol                    |
| `macos`    | `crates/platform-macos`: the IOBluetooth and audio output adapter  |
| `linux`    | `crates/platform-linux`: the BlueZ and default sink adapter        |
| `shell`    | `app/src-tauri`: the Tauri shell and tray                          |
| `ui`       | `app/src`: the React UI                                            |
| `ci`       | `.github/workflows` (with a type other than `ci`, e.g. `fix(ci)`)  |
| `deps`     | Dependency bumps, usually with `build`                             |
| `adr`      | `docs/adr`                                                         |
| `glossary` | `GLOSSARY.md`                                                      |

## Breaking changes

Mark a change that breaks a public interface (a crate API another crate uses, the shell-to-UI commands and events, saved settings) with `!` before the colon, and explain the break in a `BREAKING CHANGE:` footer. Use both together.

## Footers

- `Refs: #<issue>` links a GitHub issue the commit works towards; `Closes: #<issue>` for the commit that finishes it.
- `BREAKING CHANGE: <what breaks and how to migrate>`.
- Keep `Co-Authored-By:` trailers as the last footers.

## Vocabulary

Name domain concepts with the terms in `GLOSSARY.md`, never the synonyms it lists under _Avoid_: "Noise Control Mode", not "ANC mode"; "Ambient", not "transparency"; "Attach", not "connect" when the control link is meant.

## Examples

```
feat(sony): set the WH-1000XM5's Ambient Level

Closes: #9
```

```
fix(macos): let a Device settle for 2 s before opening its control link

Opening RFCOMM straight after the OS reports a Device as Connected
times out about one time in three, and the Device then refuses the
control link until it reconnects.
```

```
feat(core)!: report the Chosen Mode and Effective Mode separately

BREAKING CHANGE: the noise_control update now carries chosen and
effective fields in place of mode; the UI reads both.
```

```
ci: build and bundle the Linux app as a deb and an AppImage
```

```
docs(glossary): define Primary Device
```

## Corrections

| Instead of                                              | Write                                                  | Because                                                    |
| ------------------------------------------------------- | ------------------------------------------------------ | ---------------------------------------------------------- |
| `Fixed the ANC toggle on Buds.`                         | `fix(samsung): set Noise Cancelling on Buds4 Pro`      | Missing type, past tense, trailing period, glossary-avoided terms |
| `feat(misc): updates`                                   | `refactor(core): extract Device::unidentified`         | Vague scope and description; the change adds no behaviour  |
| `feat: Connect to the device on linux and also fix the tray label` | Two commits: `feat(linux): let the core Attach to a Device through BlueZ` and `fix(shell): label the tray from one place` | One change per commit; "connect" where "Attach" is meant |

## Exempt messages

The hook passes messages git writes itself: `Merge ...`, `Revert "..."`, and `fixup!` / `squash!` / `amend!` prefixes. Squash the `fixup!` commits away before a pull request merges.
