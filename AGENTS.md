# 项目约定

- 开发提交是进度检查点，保持 13 文件、400 行，只执行暂存快检；功能可以尚未完成。
- Herman 决定分支的合入顺序。被点名的作者在独立工作树获取最新 `origin/main`，
  rebase 并解决冲突，对最终提交运行 `./scripts/check` 和改动相关测试。
  通过后用 `git merge --ff-only` 快进干净的本地 `main`，普通推送并读回远端。
  远端前进时回到作者分支重新 rebase 和验证。日常合入无需 PR 或远端 CI。
- 主线禁止强推和删除。作者步骤见 `Docs/INTEGRATION.md`。
- 普通开发保持现有版本。Herman 安排发布时，用 `./scripts/check --prepare-release` 冻结版本。
- 发布流水线固定 `main` 的冻结提交，核对同一源码的 Linux 和 macOS 验证，
  再核对包完整性、隔离启动及故障场景。不可变 Release 就绪后一次更新两个候选渠道。
- 发布失败保留上一可用渠道，由 GitHub 工单保存准确源码、失败阶段和运行链接。
  独立修复分支使用 `Release-Fix: #编号`，成功读回双平台渠道后确认恢复。
- 安装、客户端重载及目标环境功能验收继续独立，不计入合并或发布阶段耗时。

- 代码应当自解释：优先用清晰的命名、合适的类型、简单的函数结构和测试表达程序行为，不依赖注释解释代码。
- 做任务时把要做的事情做正确，不要靠打补丁凑合。旧代码自己的行为写错了，就改正它。不要在旧代码旁边补一块，让旧代码继续跑去完成另一件事。
- 每次会话结束或任何开发任务结束前，必须运行 `./scripts/check`，并根据脚本提示处理检查结果，直到脚本检查通过。当前会话只做只读调查，并且没有修改项目、也没有留下新文件时，可以不运行。
- 完整候选、暂存快照和源码包验证统一使用 `./scripts/validate`。用 `--plan` 预览、`--report` 回看。环境、缓存归属、报告和退出规则见 `Docs/VALIDATION.md`。
- 每次提交前，准备版本、选择暂存内容并审阅后，必须运行 `./scripts/check --pre-commit`；
- Agent 自主决定本地 Git 提交，无须逐次向用户申请授权；继续遵守审阅、测试、提交规模和版本规则。推送与部署按对应任务的授权范围执行。

## 会话看板验收

- 当前仓库全部业务功能为 Rust Codex 会话看板。原 Notion 功能保留在 Git 历史。
- 开发前读 `acceptance/requirements.json`、`acceptance/README.md` 和 `Docs/MIGRATION.md`。
- 保留 31 条需求编号、需求文件、独立参考模型和锁；不得降低断言、跳过失败或测试旧实现来签收新实现。
- 验收阶段完成判定执行 `python3 scripts/verify_acceptance.py`。仅 `--scope automated` 通过不算完整验收完成。
- 缺少目标环境证据保持待验证。完整验收退出 0、`release_ready=true` 且证据内容经审阅，才可宣称完整验收完成；不阻止符合上文条件的合并阶段交付。

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

## 隔离开发入口

- 常规开发优先在 Mac 的独立工作树运行 `./scripts/dev setup` 和 `./scripts/dev start`；
  用 `status` 查看本任务，用 `stop` 停止本任务。详见 `Docs/MAC-DEVELOPMENT.md`。
- 默认只连接本任务的原生依赖 fixture。普通开发不得安装、重载正式插件或操作用户日用
  Codex 实例；有明确部署授权的交付任务按其授权执行。
- 浏览器验证使用独立 profile 和随机调试端口，结束时只清理自己创建的进程。
- fixture 验证不替代真实 Codex 验收。真实客户端验收单独安排，不因开发任务并行而并发修改
  同一客户端实例；缺少目标环境证据继续保持待验证。

## 客户端后台连接

- 业务连接统一经过 `App::connection`；独立 `StdioReader` 只用于历史补读，不作为实时服务。
- Mac 共享入口及自动验证见 `Docs/SHARED-DESKTOP.md`。端点继承、进程归属、重连及错误后台拒绝由连接层和门禁负责，业务任务无须自行挑选或验证后台。
- 不从 `daemon version` 的 socketPath 推断当前客户端后台，不用换地址或关闭校验来绕过连接错误。
