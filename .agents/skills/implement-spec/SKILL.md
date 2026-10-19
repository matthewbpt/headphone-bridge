---
name: implement-spec
description: "Implement the result of /to-spec and /to-tickets in code."
disable-model-invocation: true
---

You have been provided a spec. This spec should have tickets associated with it, describing how to implement the spec. Work them through the issue tracker as `docs/agents/issue-tracker.md` describes.

The goal is the entire spec implemented on a single **integration branch**, landing as one PR that closes the spec and every ticket.

The tickets are not a list of steps. They are a **task graph** with blocking relationships between them. This means there is always a **frontier** of tickets which are ready to be grabbed.

Communication to and from subagents should be sparse. Communicate primarily through **context pointers**: to the spec, tickets, research notes, and previous commits. Don't duplicate information already available via pointers.

This flow is the default. If a memory or note asks for a different one (stacked PRs, per-ticket PRs, implementer self-review), ask the user before applying it. Decisions made earlier in this conversation outrank memories.

## You coordinate

You are the **orchestrator**: you plan, brief, merge, review and report, and implementers change the code. That includes user follow-ups that change code (bug reports, review fixes): hand each to an implementer, reusing a finished one when its context helps. The one exception is a single-slice spec (below).

Start subagents as `AGENTS.md` § Herdr describes. Run **at most 3 implementers at once**: this machine has 14 GB and ran out of memory with more.

Ask any decision that blocks a later step as its own short message, never as a bullet in a status report.

## Steps

1. Read the spec and tickets to understand the task graph. Done when you know each ticket's blockers, the frontier, and which tickets are **human-in-the-loop** (they need the user and a Device: captures, hardware checks). A spec with no child tickets that fits one context window goes to [Single-slice spec](#single-slice-spec) instead.

2. (optional) Use an **exploration subagent** to conduct any exploration required by the tickets: relevant codebase files or external documentation. It saves its markdown notes in the scratchpad, outside the repo, where every later subagent can read them. This lets implementers focus on implementation rather than exploration.

3. Create the integration branch from `main`.

4. Start an **implementer** for each frontier ticket, each in its own worktree on its own branch based on the integration branch tip. Write each one a brief per [implementer-brief.md](implementer-brief.md) and prompt it with the brief's path. Human-in-the-loop tickets start before or alongside the first implementers, because their findings can change other tickets; tell the user which pane to talk to for each. Done when every frontier ticket has an implementer, or waits for a free slot.

5. When an implementer reports done, fast-forward the integration branch onto its branch (`git merge --ff-only`). Use a **merger subagent** only when the fast-forward fails. Push, then:
   - after the first merge, open a draft PR whose body starts `Closes #<spec>, closes #<ticket>, …` for the spec and every ticket;
   - after every push, refresh the PR body (the `pr` skill writes it).

   Done when the ticket's commits are on the pushed integration branch and the PR body covers them.

6. Whenever `main` moves, merge it into the integration branch before starting new implementers. Start implementers for any tickets the merge unblocked (step 4). Done when every ticket is merged.

7. Call the Skill tool with `code-review` on the integration branch. Give all its findings to one implementer. The integration branch is already pushed and force-push is blocked, so fixes land as new commits; never amend or rebase it. Merge them as in step 5. Done when the latest `code-review` covers every commit on the branch other than its own fixes.

8. Mark the PR ready for review and watch CI with `gh pr checks <n> --watch`; you alone watch it. Treat a red check as a ticket: read `gh run view <id> --log-failed`, rerun it if the failure is infra, otherwise send the fix to an implementer and review it per step 7. Done when every check is green.

9. Close every Herdr pane and worktree this run created (implementers, mergers, reviewers), and delete any memory the run wrote for itself. Done when none of them remain.

10. Report the PR, listing any acceptance criteria that couldn't be checked locally (e.g. emulator, macOS app).

## Single-slice spec

Implement it yourself on the current branch: call the Skill tool with `tdd`, then `code-review`, and fix the findings in-session. Open the PR ready for review, its body closing the spec, then finish with steps 8–10. There is no integration branch, implementer, merger or worktree.

## Per-ticket PRs

Only when the user asks for a PR per ticket. Decide the merge shape before the first implementer: this repo squash-merges and `gh stack` needs a **chain**, so order the frontier into one chain that respects the task graph, and base each implementer on the previous ticket's branch instead of an integration branch.
