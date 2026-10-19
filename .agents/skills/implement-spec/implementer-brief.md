# Implementer brief

Write one brief per ticket to a file in the scratchpad, and prompt the implementer with its path. Every brief holds:

- **Pointers** to the spec, the ticket, the exploration notes, and the commits or files to follow. Cite commits and files you have looked at; a ticket number recalled from memory can point at the wrong work.
- **Neighbours**: which files parallel tickets also edit, and how this ticket stays out of their way (e.g. "in `X`, touch only the include paths").
- **Base**: confirm the worktree is on the integration branch tip, and reset onto it if not.
- **Build**: call the Skill tool with `tdd`. Run one cargo build at a time; memory is tight.
- **Commits**: each change as its own `git commit` call, never chained with `&&`, so a rejected message can't carry files into the next commit.
- **Done**: `scripts/check` passes and the integration branch tip is merged into this branch. CI is the orchestrator's to watch.
- **Scope**: if finishing needs an interface a sibling ticket owns, stop and report instead of building it. If the user sends a request about orchestration, review or other tickets, say it belongs to the orchestrator pane (name it) and carry on. Keep notes in the scratchpad; don't write to memory.

## Human-in-the-loop tickets

The brief also names the orchestrator's Herdr agent, and ends: when done, report with `herdr agent prompt <orchestrator> "<summary>"`, recording any correction the findings force on the spec or tickets in that report.
