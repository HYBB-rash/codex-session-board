# 项目约定

- 代码应当自解释：优先用清晰的命名、合适的类型、简单的函数结构和测试表达程序行为，不依赖注释解释代码。
- 做任务时把要做的事情做正确，不要靠打补丁凑合。旧代码自己的行为写错了，就改正它。不要在旧代码旁边补一块，让旧代码继续跑去完成另一件事。
- 每次会话结束或任何开发任务结束前，必须运行 `./scripts/check`，并根据脚本提示处理检查结果，直到脚本检查通过。
- 每次提交前，准备版本、选择暂存内容并审阅后，必须运行 `./scripts/check --pre-commit`；
- Agent 自主决定本地 Git 提交，无须逐次向用户申请授权；继续遵守审阅、测试、提交规模和版本规则。推送与部署按对应任务的授权范围执行。

## 插件进程

- 插件是单体程序，不启动后台服务。它跟随 CodeHost（拉起这个插件的 Codex 进程）一起启动和退出。
- 插件进程里禁止调用 `service::launch`、再执行 `serve`，或把看板放进新的进程组。编译时 `build.rs` 会拒绝这些写法。
- 会话状态留在数据库里。会话控制的形式化模型不依赖常驻进程，不要为这条守则去改那套状态。
- `codex-session-board launch` 仍是旧的受管服务入口。插件不得调用它，无关任务也不要顺手重写它。理由见 `context/decisions/plugin-monolith.md`。

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
