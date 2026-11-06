# 项目约定

- 代码应当自解释：优先用清晰的命名、合适的类型、简单的函数结构和测试表达程序行为，不依赖注释解释代码。
- 每次会话结束或任何开发任务结束前，必须运行 `./scripts/check`，并根据脚本提示处理检查结果，直到脚本检查通过。
- 每次提交前，准备版本、选择暂存内容并审阅后，必须运行 `./scripts/check --pre-commit`；

## 会话看板验收

- 当前仓库全部业务功能为 Rust Codex 会话看板。原 Notion 功能保留在 Git 历史。
- 开发前读 `acceptance/requirements.json`、`acceptance/README.md` 和 `Docs/MIGRATION.md`。
- 保留 31 条需求编号、需求文件、独立参考模型和锁；不得降低断言、跳过失败或测试旧实现来签收新实现。
- 完成判定执行 `python3 scripts/verify_acceptance.py`。仅 `--scope automated` 通过不算完成。
- 缺少目标环境证据保持待验证。完整验收退出 0、`release_ready=true` 且证据内容经审阅，才可宣称完成。

<!-- project-context:start -->
## Project context

At the start of a task, read `context/README.md`, `context/PROJECT.yaml`, and `context/STATE.yaml`. Read only the active task, relevant handoff, and task-relevant historical records. Do not load all history by default.

For sequential work, create an active task card. On interruption, write a handoff. On completion, the sole writer archives the task, updates `context/STATE.yaml` and the journal, then runs `sh scripts/verify_context.sh`. In coordinated parallel work, workers write only their uniquely named task, handoff, or result record; the coordinator performs registration, archiving, shared-state updates, and verification.

The context system is project-local. Do not change external systems, production data, credentials, or live configuration unless the task explicitly authorizes it.

## Parallel execution

Read-only agents may share a worktree. Concurrent write tasks use one separate Git worktree and branch per task; a branch alone does not isolate two agents in one directory. In a shared coordinator worktree, workers must not checkout, reset, commit, or rewrite shared files. The coordinator alone updates `context/STATE.yaml`, `current_phase`, verification metadata, the journal, and task registration. Workers write only uniquely named task, handoff, or result files and return them to the coordinator. A lock can protect a short context transaction, but it cannot replace worktree isolation.

If Git worktrees or an equivalent isolated copy are unavailable, write tasks run serially; read-only tasks may still run concurrently. Before creating a worktree, record the base commit and confirm that the coordinator worktree is clean. A branch without a separate worktree is not isolation.

Active task cards declare `owner`, `execution_mode`, `allowed_write_set`, `base_commit`, `worktree_path`, and `coordinator`. `STATE.yaml`, `current_phase`, verification metadata, journal entries, and task registration are coordinator-owned during coordinated work.
<!-- project-context:end -->
