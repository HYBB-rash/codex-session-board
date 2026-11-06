# Project Context

This directory maintains the project's durable working context. Read it progressively; do not load every historical record at startup.

## Startup order

1. `AGENTS.md`
2. `context/README.md`
3. `context/PROJECT.yaml`
4. `context/STATE.yaml`
5. The relevant active task and handoff
6. Only the history, decisions, or source files required by the current task

## Ownership

| Path | Purpose |
| --- | --- |
| `PROJECT.yaml` | Stable project metadata, features, paths, and template version |
| `STATE.yaml` | Current volatile snapshot |
| `tasks/active/` | Work currently in progress |
| `tasks/completed/` | Archived task cards |
| `handoffs/` | Resumable interruption records |
| `journal/` | Short chronological execution notes |
| `decisions/` | Durable decisions and their rationale |
| `results/` | Worker result records returned to the coordinator |

Dates use the timezone and local-calendar policy declared in `PROJECT.yaml`. The canonical validation entry is `sh scripts/verify_context.sh`.

The initializer creates the structure without inventing business history. Project-specific rules belong in the project-owned part of `AGENTS.md` and in the relevant context records. For a single-writer task, that writer may update the task lifecycle and shared state; in coordinated parallel work, the coordinator performs those shared updates.

## Parallel execution

Read-only analysis may share a worktree. Concurrent write tasks use separate Git worktrees and branches. The coordinator owns `STATE.yaml`, `current_phase`, verification metadata, the journal, and task registration; workers write uniquely named task or handoff records and do not rewrite shared state. A lock may serialize a short state update but cannot solve source-file, Git-index, build-output, or external-side-effect conflicts.

If worktrees or an equivalent isolated copy are unavailable, write tasks must run serially. Before creating a worktree, record the base commit and confirm the coordinator worktree is clean; a branch without a separate worktree is not isolation. Active task cards declare their owner, execution mode, allowed write set, base commit, worktree path, and coordinator.
