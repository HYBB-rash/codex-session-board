# A 保存一张卡片

- owner: 本聊天作者
- execution_mode: isolated_write
- allowed_write_set: src/app.rs, src/app/remote.rs, index.html, tests/native_authority/entrypoints.rs, tests/native_authority/remote_isolation.rs, tests/remote_sessions/routes.rs, scripts/check-blocked-optional.mjs, scripts/check-transfer-feedback.mjs, scripts/check-light-ui.mjs, acceptance/README.md, context/tasks/active/refactor-a-card-save-20261121.md, context/results/refactor-a-card-save-20261121.md
- base_commit: b859880165908a848a4982d2cfd9b722e9aeb912
- rebased_base_commit: 3de9e0f00f5458eda255b59e81cc54e63818f071
- worktree_path: /Users/hongyong/.codex/worktrees/51f3/codex-session-board
- coordinator: Herman / source chat 01a1fc2d-af6f-7d50-9759-33835b51eaeb
- branch: codex/refactor-a-card-save
- status: author_ready

## 范围和成败标准

保存接口返回后端实际确认的卡片，保存回执与随后快照共用业务投影。
页面只按主机和会话身份采用回执，保留临时拖动、等待提示和失败草稿。
不改归档批次、测试清单机制、Vue、列规则、版本及 31 条需求、独立模型和锁。
不推进 main，不推送，不部署，不操作日用客户端。

## 首件

隔离 `./scripts/dev setup/start` 后，在真实网页拖入已阻塞，输入两个空格。
并排记录真实请求、回执、刷新前页面卡片和最终快照，先报告再继续实现。

## 疑点

- 当前 `persist` 用请求 note、version+1 和 followUp=false 推算卡片，纯空白可能在刷新前显示错误。
- managed remote 通过 App 路由到目标主机，回执必须恢复外部复合 ID，不能用裸 UUID 匹配。
- 后端成功而确认丢失时必须重新读取；拒绝旧版本、新输入及原生位置变更仍由后端执行。

## 认领测试

- test::native_authority::entrypoints::built_binary_distinguishes_omitted_and_empty_notes_over_http_and_mcp
- test::native_authority::entrypoints::built_binary_preserves_native_position_guards_over_http_and_mcp
- test::remote_sessions::routes::remote_routes_bypass_local_authority_and_preserve_host_project_identity
- test::native_authority::remote_isolation::remote_online_disconnected_and_scanning_do_not_delay_local_http_mutations
- scripts/check-blocked-optional.mjs 的保存、同列编辑、备注省略/清空/纯空白、取消及失败草稿场景。
- scripts/check-transfer-feedback.mjs 的保存回执、未知结果读回和跨主机同 UUID 隔离场景。

完整测试 ID 以实际 test binary 枚举为准，相关既有测试补断言，不删除或放松断言。
测试归属由只读子代理核对，最终准确提交另做独立只读审阅。

## 首件结果

基线真实 Rust fixture 的请求 note 为两个空格，回执只有同步提示。
刷新前页面卡片 note 为两个空格，刷新后 note 为空串，已复现结果不一致。
四份原值及独立 Chrome 清理凭据保存在 `/private/tmp/refactor-a-first-baseline.json`。

## 交付

实现及审阅修复提交已 rebase 为 `58bee839592994c344de173ba3431008aab0115f`。
最新基线包含 B1 `dce3536b`、C1、T2 和 C2，155 项相关 Rust 测试、11 项真实备注场景、69 项回执场景、102 项页面断言和 49 项桥接断言通过。
保存回执沿用 B1 合流后的最终归列投影，独立复评无新增业务冲突或阻塞项。
独立审阅发现的未归列完成提示 P2 已复现、修复并续审，无剩余阻塞项。
本任务 fixture 已停止，`cleanup_confirmed=true`。
详见 `context/results/refactor-a-card-save-20261121.md`。
等待 Herman 点名合入。共享状态、任务归档和 journal 由协调者收尾。
