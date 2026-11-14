# 项目约定

- 开发提交是进度检查点，保持 13 文件、400 行，只执行暂存快检；功能可以尚未完成。
- PR 围绕一个明确改动审阅，相关测试与依赖在 PR 中说明；不得夹带无关功能。
- 集成候选固定来源头并审阅冲突修改，通过完整组合门禁后才能进入主线。
- 版本只在冻结发布候选时由协调者执行 `./scripts/check --prepare-release` 更新；
  开发 PR 不修改版本清单和共享 STATE，任务记录写到本任务文件。
- 发布部署已经验证的不可变候选产物；主线、候选、发布完成分别判定。
- 合并阶段完成于最终组合自动门禁通过、主线更新并产出通过隔离启动检查的不可变插件包。
  安装、客户端重载及目标环境功能验收属于独立阶段，不计入合并阶段耗时。

- 代码应当自解释：优先用清晰的命名、合适的类型、简单的函数结构和测试表达程序行为，不依赖注释解释代码。
- 做任务时把要做的事情做正确，不要靠打补丁凑合。旧代码自己的行为写错了，就改正它。不要在旧代码旁边补一块，让旧代码继续跑去完成另一件事。
- 每次会话结束或任何开发任务结束前，必须运行 `./scripts/check`，并根据脚本提示处理检查结果，直到脚本检查通过。当前会话只做只读调查，并且没有修改项目、也没有留下新文件时，可以不运行。
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
