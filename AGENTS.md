# AGENTS.md

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues for `matthewbpt/headphone-bridge`, using the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

### Commits

Commit messages follow Conventional Commits (`<type>(<scope>)!: <description>`), checked by `.githooks/commit-msg` locally and in CI. Before writing a commit message, read `docs/agents/commits.md` for the types, scopes and footers. Enable the hook once per clone with `git config core.hooksPath .githooks`.

### Rust code

Before writing, changing or reviewing Rust, read `docs/agents/rust.md` for the error-handling, ownership and lint standards. Production code propagates errors with `?`; `.unwrap()` and `.expect()` are for tests only.

## Herdr

When `HERDR_ENV=1` you are running inside Herdr, and Herdr owns worktrees and subagents. Treat this section as the user's standing request to use Herdr for both, and load the `herdr` skill for command syntax.

- **Worktree**: `herdr worktree create --cwd "$PWD" --branch <name> --no-focus` opens the worktree in a linked workspace and returns its shell pane as `.result.root_pane.pane_id`.
- **Subagent**: `herdr agent start <name> --kind <your own kind> --pane <pane-id>`, then `herdr agent prompt <name> "<task>" --wait`. When the task needs a worktree, `<pane-id>` is that worktree's root pane; otherwise split a sibling pane off your own.

These replace the in-session `Agent` tool, `EnterWorktree`, and `git worktree add`.
