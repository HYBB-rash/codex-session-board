# 项目约定

- 开发提交是进度检查点，保持 13 文件、400 行，只执行暂存快检；功能可以尚未完成。
- PR 围绕一个明确改动审阅，相关测试与依赖在 PR 中说明；不得夹带无关功能。
- 作者交付使用 `./scripts/validate --scope pr --stage submit`；完整 PR CI 与候选构建并行，
  队列在推进主线前核对最终 PR 头的实际 CI。不得用 submit 报告代替 CI 或组合门禁。
- 集成候选固定来源头并审阅冲突修改，通过完整组合门禁后才能进入主线。
- 合并协调者只处理队列、证据和机械合并；业务审阅与冲突修复由作者及独立审阅者完成。
  用户手动调度合并及作者会话，队列使用 github 反馈模式。
  不合格项在 PR 留下绑定源码的实际错误、日志和复现入口，反馈持久保存后本轮返回，
  不等待作者 ACK 或修复，不创建定时器、自动唤醒或持续合并服务。
  外部 CI 未完成是待 CI，保留候选和运行身份后返回，不能当作代码失败。
  用户再次要求继续时，使用队列的 PR 定位及 resume 入口刷新远端事实，
  复用仍有效的记录；main 漂移后重新组合，不能提升旧组合。
  批量任务跳过阻塞项并继续独立项。已有 automatic 作者任务不批量迁移或删除。
  作者 CLI 与队列共用相同冻结检查器版本及运行平台；作者开发使用另一个独立工作树。
  作者返回新头后，旧审阅和旧验证不得继续放行；依赖项等待，独立项可以继续。
- 版本只在冻结发布候选时由协调者执行 `./scripts/check --prepare-release` 更新；
  开发 PR 不修改版本清单和共享 STATE，任务记录写到本任务文件。
- 发布部署已经验证的不可变候选产物；主线、候选、发布完成分别判定。
- PR 合并完成于最终组合必要的编译、测试和组合门禁通过，并进入 main；
  不等待发布打包、上传或隔离启动。只通过 cargo check 不足以合并。
  确认 main 和 PR 终态后可报告“已合并，发布中”并结束本轮；
  额外收尾及记录远端同步分别记录，不以等待发布或终态清理维持会话。
- main 更新后由现有 release CLI 的自动流水线发布固定源码：双平台包完整性、
  隔离启动及故障场景通过后，原子更新两个候选渠道；成功才算发布完成。
  在跑发布不取消，等待中的旧请求合并为最新主线；记录更新复用原实现包。
- 发布失败保留上一可用渠道，状态为“代码已合并、尚未发布”，通过 GitHub 工单和
  原 PR 反馈交作者修复。未恢复时只合入引用故障的修复；确认门禁遗漏须补合并前回归。
- 安装、客户端重载及目标环境功能验收继续独立，不计入合并或发布阶段耗时。

- 代码应当自解释：优先用清晰的命名、合适的类型、简单的函数结构和测试表达程序行为，不依赖注释解释代码。
- 做任务时把要做的事情做正确，不要靠打补丁凑合。旧代码自己的行为写错了，就改正它。不要在旧代码旁边补一块，让旧代码继续跑去完成另一件事。
- 每次会话结束或任何开发任务结束前，必须运行 `./scripts/check`，并根据脚本提示处理检查结果，直到脚本检查通过。当前会话只做只读调查，并且没有修改项目、也没有留下新文件时，可以不运行。
- PR、完整候选、暂存快照和源码包验证统一使用 `./scripts/validate`；用 `--plan` 预览、`--report` 回看。环境、缓存归属、报告和退出规则见 `Docs/VALIDATION.md`。
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
